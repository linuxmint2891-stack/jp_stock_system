use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone, Utc};
use clap::Parser;
use jp_stock_system::alpha::{alpha_a, alpha_b};
use jp_stock_system::api::jquants::fetch_daily_bars;
use jp_stock_system::api::yahoo::fetch_ohlc;
use jp_stock_system::api::yahoo::fetch_yahoo_bulk;
use jp_stock_system::utils::get_unique_codes;
use jp_stock_system::utils::settings::Settings;
use polars::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

use google_drive3::api::File;
use google_drive3::hyper;
use google_drive3::hyper_rustls;
use google_drive3::DriveHub;

const PARQUET_PATH: &str = "data/processed_market_data.parquet";

#[derive(Parser)]
struct Args {
    /// 3ヶ月に1回のメンテナンスモード（全銘柄の過去分をYahooから詳細同期）
    #[arg(long)]
    maintenance: bool,

    /// 銘柄コードの範囲指定（例: "1000-3000"）
    #[arg(long)]
    range: Option<String>,
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

    let start_date = if args.maintenance {
        last_date.succ_opt().unwrap_or(last_date)
    } else {
        last_date.succ_opt().unwrap_or(today)
    };

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let mut all_new_rows = Vec::new();
    let expected_latest_date = match today.weekday().number_from_monday() {
        6 => today - Duration::days(1),
        7 => today - Duration::days(2),
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

    // --- STEP 2: Yahoo Zone (Direct or Bulk) ---
    let yahoo_start_date = if api_key.trim().is_empty() {
        start_date
    } else if current_date > jquants_end_date {
        current_date
    } else {
        jquants_end_date
    };

    if yahoo_start_date <= today {
        let mut codes = get_unique_codes(PARQUET_PATH)?;
        if codes.is_empty() {
            anyhow::bail!("Yahoo同期対象の銘柄コードがParquetに存在しません");
        }

        if let Some(ref range_str) = args.range {
            let parts: Vec<&str> = range_str.split('-').collect();
            if parts.len() == 2 {
                let start: u32 = parts[0].parse().unwrap_or(0);
                let end: u32 = parts[1].parse().unwrap_or(9999);

                codes.retain(|code| {
                    let num: u32 = code
                        .chars()
                        .filter(|c| c.is_ascii_digit())
                        .collect::<String>()
                        .parse()
                        .unwrap_or(0);
                    num >= start && num <= end
                });
                println!(
                    "🎯 [Range Filter] {} (対象: {} 銘柄)",
                    range_str,
                    codes.len()
                );
            }
        }

        let is_up_to_date_pre_day = match today.weekday().number_from_monday() {
            1 => (today - last_date).num_days() <= 3,
            _ => (today - last_date).num_days() <= 1,
        };

        // メンテナンスモード指定がなく、かつ前営業日までのデータがすでにある場合のみバルク取得（デイリーモード）を使用する
        let use_bulk = !args.maintenance && is_up_to_date_pre_day;

        if !use_bulk {
            if args.maintenance {
                println!(
                    "🧹 Phase 2: Running full maintenance sync for {} codes...",
                    codes.len()
                );
            } else {
                println!(
                    "🔄 [Auto-Switch] Parquet最終日 ({}) と本日 ({}) の間にギャップがあるため、履歴同期モードを実行します...",
                    last_date, today
                );
            }
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
            // 🚀 デイリーモード: 100件ずつ一括取得
            println!("🚀 Phase 2: Running lightweight bulk sync for targets...");

            // 土日の場合は最新の営業日（金曜日）の日付を割り当て、平日は今日の日付にする
            let target_date = if today.weekday().number_from_monday() == 6 {
                today - Duration::days(1)
            } else if today.weekday().number_from_monday() == 7 {
                today - Duration::days(2)
            } else {
                today
            };

            for chunk in codes.chunks(100) {
                let symbols: Vec<String> = chunk
                    .iter()
                    .map(|c| {
                        if c.len() == 4 {
                            format!("{}.T", c)
                        } else {
                            format!("{}.T", &c[..4])
                        }
                    })
                    .collect();

                if let Ok(results) = fetch_yahoo_bulk(&client, &symbols).await {
                    for (code, price, volume) in results {
                        all_new_rows.push((
                            target_date.to_string(),
                            code,
                            price,
                            price * volume,
                            volume,
                        ));
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }
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
    // サービスアカウント鍵を使用する。OAuth の tokencache.json は認証鍵ではないため、
    // ここで読み込まない（yup-oauth2 9 系では旧形式のキャッシュを読むと JSONToken の
    // 形式エラーになる）。
    let sa_key = load_service_account_key().await?;

    let auth = yup_oauth2::ServiceAccountAuthenticator::builder(sa_key)
        .build()
        .await
        .map_err(|e| anyhow::anyhow!("Google サービスアカウント認証の初期化に失敗しました: {e}"))?;

    let connector = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .expect("Native roots could not be loaded")
        .https_or_http()
        .enable_http1()
        .build();

    let client = hyper::Client::builder().build(connector);
    let hub = DriveHub::new(client, auth);

    // 2. アップロード設定
    let file_data = fs::File::open(file_path)?;

    // 3. 親フォルダIDの取得（環境変数から）
    let folder_id = std::env::var("GDRIVE_UPLOAD_FOLDER_ID").ok();

    // 4. 既存ファイルの検索（指定フォルダがある場合は parents 条件を追加）
    let query = if let Some(ref fid) = folder_id {
        format!(
            "name = '{}' and '{}' in parents and trashed = false",
            file_name, fid
        )
    } else {
        format!("name = '{}' and trashed = false", file_name)
    };

    let (_, file_list) = hub
        .files()
        .list()
        .q(&query)
        .add_scope(google_drive3::api::Scope::Full)
        .doit()
        .await
        .map_err(|e| anyhow::anyhow!(
            "Google Drive の既存ファイル検索に失敗しました: {e}. \
             フォルダ ID が正しいこと、およびサービスアカウントにフォルダを共有していることを確認してください。"
        ))?;

    let existing_file_id = file_list
        .files
        .and_then(|f| f.get(0).and_then(|f| f.id.clone()));

    match existing_file_id {
        Some(id) => {
            println!("🔄 上書きアップロード中 (ID: {})...", id);
            hub.files()
                .update(File::default(), &id)
                .add_scope(google_drive3::api::Scope::Full)
                .upload(file_data, "application/octet-stream".parse().unwrap())
                .await
                .map_err(|e| {
                    anyhow::anyhow!("Google Drive の上書きアップロードに失敗しました: {e}")
                })?;
            println!("✅ 上書き成功！");
        }
        None => {
            println!("🆕 新規アップロード中...");
            let mut file_meta = File::default();
            file_meta.name = Some(file_name.to_string());
            if let Some(fid) = folder_id {
                file_meta.parents = Some(vec![fid]);
            }

            hub.files()
                .create(file_meta)
                .add_scope(google_drive3::api::Scope::Full)
                .upload(file_data, "application/octet-stream".parse().unwrap())
                .await
                .map_err(|e| {
                    anyhow::anyhow!("Google Drive の新規アップロードに失敗しました: {e}")
                })?;
            println!("✅ 新規作成成功！");
        }
    }

    Ok(())
}

/// サービスアカウント鍵の取得元（優先順）:
/// 1. `GCP_SA_KEY` / `GDRIVE_SECRET_JSON`（JSON 本文またはファイルパス）
/// 2. `data/API_Key/credentials.json`（このプロジェクトの標準配置）
/// 3. プロジェクト直下の `credentials.json`
async fn load_service_account_key() -> anyhow::Result<yup_oauth2::ServiceAccountKey> {
    for variable in ["GCP_SA_KEY", "GDRIVE_SECRET_JSON"] {
        if let Ok(value) = std::env::var(variable) {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }

            if value.starts_with('{') {
                return yup_oauth2::parse_service_account_key(value.to_owned()).map_err(|e| {
                    anyhow::anyhow!(
                        "{variable} はサービスアカウント鍵 JSON として読み込めません: {e}"
                    )
                });
            }

            let path = Path::new(value);
            if !path.exists() {
                anyhow::bail!("{variable} で指定されたファイルが見つかりません: {value}");
            }
            return yup_oauth2::read_service_account_key(path)
                .await
                .map_err(|e| {
                    anyhow::anyhow!("{variable} のサービスアカウント鍵を読み込めません: {e}")
                });
        }
    }

    let candidates = [
        PathBuf::from("data/API_Key/credentials.json"),
        PathBuf::from("credentials.json"),
    ];
    for path in candidates {
        if path.exists() {
            println!("🔑 Google サービスアカウント鍵を使用: {}", path.display());
            return yup_oauth2::read_service_account_key(&path)
                .await
                .map_err(|e| {
                    anyhow::anyhow!(
                        "サービスアカウント鍵 {} を読み込めません: {e}",
                        path.display()
                    )
                });
        }
    }

    anyhow::bail!(
        "Google Drive 用サービスアカウント鍵が見つかりません。\
         data/API_Key/credentials.json を配置するか、GCP_SA_KEY または GDRIVE_SECRET_JSON を設定してください。\
         OAuth クライアント鍵（client_secret.json）や tokencache.json はここでは使用できません。"
    )
}
