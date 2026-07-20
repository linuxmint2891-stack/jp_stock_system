use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone, Utc};
use clap::Parser;
use jp_stock_system::alpha::{alpha_a, alpha_b};
use jp_stock_system::api::jquants::fetch_daily_bars;
use jp_stock_system::api::yahoo::fetch_ohlc;
use jp_stock_system::utils::get_unique_codes;
use jp_stock_system::utils::settings::Settings;
use polars::prelude::*;
use std::fs;
use std::path::Path;
use google_drive3::{api::File, DriveHub};
use google_drive3::hyper;
use google_drive3::hyper_rustls;
use yup_oauth2::{InstalledFlowAuthenticator, InstalledFlowReturnMethod};

const PARQUET_PATH: &str = "data/processed_market_data.parquet";

#[derive(Parser)]
struct Args {
    /// 3ヶ月に1回のメンテナンスモード（全銘柄の過去分をYahooから詳細同期）
    #[arg(long)]
    maintenance: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();
    let settings = Settings::new()?;
    let api_key = &settings.jquants.api_key;

    if args.maintenance {
        println!("🧹 [Mode] メンテナンスモード (フル同期を実行します)");
    } else {
        println!("🚀 [Mode] デイリーモード (超軽量・最新分のみ同期します)");
    }

    println!("🚀 Starting Hybrid Data Sync (J-Quants + Yahoo Finance)...");

    // 1. 既存の Parquet から最新日付を取得
    let mut last_date = NaiveDate::from_ymd_opt(2024, 3, 19).unwrap();
    let file_exists = Path::new(PARQUET_PATH).exists();

    if file_exists {
        if let Ok(df_last) = LazyFrame::scan_parquet(PARQUET_PATH, Default::default())?
            .select([col("Date").max()])
            .collect()
        {
            if let Ok(series) = df_last.column("Date") {
                if let Ok(ca) = series.str() {
                    if let Some(date_val) = ca.get(0) {
                        if let Ok(parsed_date) = NaiveDate::parse_from_str(date_val, "%Y-%m-%d") {
                            last_date = parsed_date;
                            println!("📅 Last date in Parquet: {}", last_date);
                        }
                    }
                }
            }
        }
    }

    // 2. 同期範囲の決定
    let today = Local::now().naive_local().date();

    // 🔥【強力なガード】デイリーモードかつ、すでに最新データ（今日か昨日）がある場合は即終了！
    if !args.maintenance && file_exists {
        let gap_days = (today - last_date).num_days();
        // 土日の場合は金曜日（2日前〜3日前）で止まるため、ギャップが2日以内なら最新とみなす
        if gap_days <= 1 || (today.weekday().number_from_monday() > 5 && gap_days <= 3) {
            println!("✨ [Skip] データはすでに最新状態です (Parquet最終日: {} / 本日: {})。処理を終了します。", last_date, today);
            return Ok(());
        }
    }

    let start_date = if args.maintenance {
        last_date.succ_opt().unwrap_or(last_date)
    } else {
        last_date.succ_opt().unwrap_or(today) // 既存の最後の日の翌日から同期スタート
    };

    if start_date > today && !args.maintenance {
        println!("✨ No new data to update (Last date is {}).", last_date);
        return Ok(());
    }

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let mut all_new_rows = Vec::new();
    // 日次同期で「取得できなかった」を「最新」と誤認しないための基準日。
    let expected_latest_date = match today.weekday().number_from_monday() {
        6 => today - Duration::days(1), // 土曜は金曜終値まで
        7 => today - Duration::days(2), // 日曜は金曜終値まで
        _ => today,
    };

    // --- STEP 1: J-Quants Zone (Bulk update) ---
    let jquants_end_date = today - Duration::days(85);
    let mut current_date = start_date;

    if current_date < jquants_end_date && !api_key.trim().is_empty() {
        println!(
            "📊 Phase 1: Syncing up to {} using J-Quants Bulk API...",
            jquants_end_date
        );

        while current_date < jquants_end_date {
            if current_date.weekday().number_from_monday() > 5 {
                current_date += Duration::days(1);
                continue;
            }

            println!(
                "🚀 Fetching bulk data for {} from J-Quants...",
                current_date
            );
            match fetch_daily_bars(&client, api_key, &current_date).await {
                Ok(bars) => {
                    if !bars.is_empty() {
                        println!("✅ Received {} quotes.", bars.len());
                        for bar in bars {
                            let code = bar["Code"].as_str().unwrap_or("").to_string();
                            let date = bar["Date"].as_str().unwrap_or("").to_string();
                            let close = bar["AdjustmentClose"]
                                .as_f64()
                                .or_else(|| bar["AdjC"].as_f64())
                                .unwrap_or(0.0);
                            let volume = bar["AdjustmentVolume"]
                                .as_f64()
                                .or_else(|| bar["AdjVo"].as_f64())
                                .unwrap_or(0.0);
                            let turnover = bar["TurnoverValue"]
                                .as_f64()
                                .or_else(|| bar["Va"].as_f64())
                                .unwrap_or(0.0);

                            if !code.is_empty() {
                                all_new_rows.push((date, code, close, turnover, volume));
                            }
                        }
                    }
                }
                Err(e) => eprintln!("❌ Error fetching {}: {}", current_date, e),
            }
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            current_date += Duration::days(1);
        }
    } else if current_date < jquants_end_date {
        println!("ℹ️ J-Quants APIキーが未設定のため、J-Quantsによる履歴同期をスキップします。");
    }

    // --- STEP 2: Yahoo Zone (個別銘柄の履歴ページから取得) ---
    let yahoo_start_date = if api_key.trim().is_empty() {
        // J-Quantsを使わない構成では、Yahoo側に全対象期間を委ねる。
        start_date
    } else if current_date > jquants_end_date {
        current_date
    } else {
        jquants_end_date
    };

    if yahoo_start_date <= today {
        let codes = get_unique_codes(PARQUET_PATH)?;
        if codes.is_empty() {
            anyhow::bail!("Yahoo同期対象の銘柄コードがParquetに存在しません");
        }

        if args.maintenance {
            println!(
                "🧹 Phase 2: Running full maintenance sync for {} codes...",
                codes.len()
            );
            let yahoo_start_ts = Utc
                .from_utc_datetime(&yahoo_start_date.and_hms_opt(0, 0, 0).unwrap())
                .timestamp();

            for code in codes {
                let symbol = if code.len() == 4 {
                    format!("{}.T", code)
                } else {
                    format!("{}.T", &code[..4])
                };
                let ohlcs = fetch_ohlc(&client, &symbol, yahoo_start_ts).await;
                for ohlc in ohlcs {
                    let d = Utc
                        .timestamp_opt(ohlc.timestamp, 0)
                        .unwrap()
                        .naive_utc()
                        .date();
                    if d >= yahoo_start_date {
                        all_new_rows.push((
                            d.to_string(),
                            code.clone(),
                            ohlc.close,
                            ohlc.close * ohlc.volume,
                            ohlc.volume,
                        ));
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        } else {
            println!(
                "🚀 Phase 2: Yahoo履歴ページから {} 銘柄を順次同期します...",
                codes.len()
            );
            let yahoo_start_ts = Utc
                .from_utc_datetime(&yahoo_start_date.and_hms_opt(0, 0, 0).unwrap())
                .timestamp();
            let mut updated_symbols = 0usize;
            let mut empty_symbols = 0usize;

            for code in codes {
                let symbol = if code.len() == 4 {
                    format!("{}.T", code)
                } else {
                    format!("{}.T", &code[..4])
                };
                let ohlcs = fetch_ohlc(&client, &symbol, yahoo_start_ts).await;
                if ohlcs.is_empty() {
                    empty_symbols += 1;
                    continue;
                }

                updated_symbols += 1;
                for ohlc in ohlcs {
                    let date = Utc
                        .timestamp_opt(ohlc.timestamp, 0)
                        .single()
                        .ok_or_else(|| {
                            anyhow::anyhow!("不正なYahooタイムスタンプ: {}", ohlc.timestamp)
                        })?
                        .naive_utc()
                        .date();
                    if date >= yahoo_start_date {
                        all_new_rows.push((
                            date.to_string(),
                            code.clone(),
                            ohlc.close,
                            ohlc.close * ohlc.volume,
                            ohlc.volume,
                        ));
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }

            println!(
                "📊 Yahoo履歴取得結果: 更新 {} / データなし {} 銘柄",
                updated_symbols, empty_symbols
            );
        }
    }

    // 3. データの結合と保存
    if !all_new_rows.is_empty() {
        println!("🔗 Merging {} new rows...", all_new_rows.len());

        let dates: Vec<String> = all_new_rows.iter().map(|x| x.0.clone()).collect();
        let codes: Vec<String> = all_new_rows.iter().map(|x| x.1.clone()).collect();
        let adj_c: Vec<f64> = all_new_rows.iter().map(|x| x.2).collect();
        let va: Vec<f64> = all_new_rows.iter().map(|x| x.3).collect();
        let adj_vo: Vec<f64> = all_new_rows.iter().map(|x| x.4).collect();

        let new_df = df!(
            "Date" => dates,
            "Code" => codes,
            "AdjC" => adj_c,
            "Va" => va,
            "AdjVo" => adj_vo
        )?;

        let new_lf = new_df.lazy().with_column(lit("").alias("news_text"));

        let combined_lf = if file_exists {
            let existing_lf = LazyFrame::scan_parquet(PARQUET_PATH, Default::default())?.select([
                col("Date"),
                col("Code"),
                col("AdjC"),
                col("Va"),
                col("AdjVo"),
                col("news_text"),
            ]);
            concat([existing_lf, new_lf], UnionArgs::default())?
        } else {
            new_lf
        };

        // 重複を除去してソート
        let final_df = combined_lf
            .unique(
                Some(vec!["Date".into(), "Code".into()]),
                UniqueKeepStrategy::Last,
            )
            .sort(["Code", "Date"], SortMultipleOptions::default())
            .collect()?;

        // アルファの計算と保存
        println!("🧪 Computing Alphas...");
        let alpha_df = alpha_a::compute(final_df.clone().lazy());
        let alpha_df = alpha_b::compute(alpha_df);
        let mut final_df = alpha_df.collect()?;
 
        let file = fs::File::create(PARQUET_PATH)?;
        ParquetWriter::new(file).finish(&mut final_df)?;
        println!(
            "✅ Parquet updated successfully. Total rows: {}",
            final_df.height()
        );

        // --- STEP 4: Google Drive Upload ---
        println!("☁️ Starting Google Drive upload...");
        upload_to_gdrive(PARQUET_PATH, "processed_market_data.parquet").await?;
        println!("✅ Google Drive sync completed.");
     } else {
        if last_date < expected_latest_date {
            anyhow::bail!(
                "Yahooから新規データを取得できませんでした。Parquet最終日: {} / 必要な最終日: {}",
                last_date,
                expected_latest_date
            );
        }
        println!("✨ No new rows fetched. Database is up to date.");
    }

    Ok(())
}

async fn upload_to_gdrive(file_path: &str, file_name: &str) -> anyhow::Result<()> {
    // 1. OAuth2.0 認証
    let secret = if let Ok(val) = std::env::var("GDRIVE_SECRET_JSON") {
        if val.trim().starts_with('{') {
            yup_oauth2::parse_application_secret(val)?
        } else {
            let path = std::path::Path::new(&val);
            if!path.exists() {
                anyhow::bail!("GDRIVE_SECRET_JSON で指定されたファイルが見つかりません: {}", val);
            }
            yup_oauth2::read_application_secret(val).await?
        }
    } else {
        let default_path = "client_secret.json";
        let path = std::path::Path::new(default_path);
        if!path.exists() {
            anyhow::bail!("client_secret.json が見つかりません。環境変数 GDRIVE_SECRET_JSON を設定してください。");
        }
        yup_oauth2::read_application_secret(default_path)
            await
            map_err(|e| anyhow::anyhow!("client_secret.json の読み込みに失敗しました: {}", e))?
    };

    if let Ok(cache_json) = std::env::var("GDRIVE_TOKEN_CACHE") {
        fs::write("tokencache.json", cache_json)?;
    }

    let auth = InstalledFlowAuthenticator::builder(
        secret,
        InstalledFlowReturnMethod::HTTPRedirect,
    ).persist_tokens_to_disk("tokencache.json")
     build()
     await?;

    let scopes = &["https://www.googleapis.com/auth/drive"];
    auth.token(scopes).await.map_err(|e| {
        anyhow::anyhow!("認証に失敗しました: {}. CI環境の場合は tokencache.json が必要です。", e)
    })?;

    let connector = hyper_rustls::HttpsConnectorBuilder::new()
        with_native_roots()
        expect("Native roots could not be loaded")
        https_or_http()
        enable_http1()
        build();

    let client = hyper::Client::builder().build(connector);
    let hub = DriveHub::new(client, auth);

    // 2. アップロード設定
    let file_data = fs::File::open(file_path)?;

    // 3. 既存ファイルの検索
    let query = format!("name = '{}' and trashed = false", file_name);
    let (_, file_list) = hub.files().list().q(&query)
        add_scope(google_drive3::api::Scope::Full)
        doit().await?;

    let existing_file_id = file_list.files.and_then(|f| f.get(0).and_then(|f| f.id.clone()));

    match existing_file_id {
        Some(id) => {
            println!("🔄 上書きアップロード中 (ID: {})...", id);
            hub.files().update(File::default(), &id)
                add_scope(google_drive3::api::Scope::Full)
                upload(file_data, "application/octet-stream".parse().unwrap())
                await?;
            println!("✅ 上書き成功！");
        },
        None => {
            println!("🆕 新規アップロード中...");
            let mut file_meta = File::default();
            file_meta.name = Some(file_name.to_string());
            hub.files().create(file_meta)
                add_scope(google_drive3::api::Scope::Full)
                upload(file_data, "application/octet-stream".parse().unwrap())
                await?;
            println!("✅ 新規作成成功！");
        }
    }

    Ok(())
}
