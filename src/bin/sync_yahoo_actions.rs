use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, TimeZone, Timelike, Utc};
use clap::Parser;
use jp_stock_system::alpha::{alpha_a, alpha_b};
use jp_stock_system::api::jquants::fetch_daily_bars;
use jp_stock_system::api::yahoo::fetch_ohlc;
use jp_stock_system::api::yahoo::fetch_yahoo_bulk;
use jp_stock_system::utils::get_unique_codes;
use jp_stock_system::utils::settings::Settings;
use polars::prelude::*;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

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
    let parquet_path = parquet_path(args.range.as_deref());
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
    let file_exists = Path::new(&parquet_path).exists();

    if file_exists {
        if let Ok(df_last) = LazyFrame::scan_parquet(&parquet_path, Default::default())?
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
    // GitHub Actions runner is UTC. 日本市場の営業日判定は常にJSTで行う。
    let now = jst_now();
    let today = now.date_naive();
    let expected_latest_date = latest_required_market_date(now);

    // デイリーモードでは市場終了前は前営業日まで、市場終了後は当日分までを必須とする。
    // 23:00 JST の定期実行で「昨日まであるから最新」と誤判定しないためのガード。
    if !args.maintenance && file_exists {
        if last_date >= expected_latest_date {
            println!(
                "✨ [Skip] データはすでに最新状態です (Parquet最終日: {} / 必要な最終日: {})。処理を終了します。",
                last_date, expected_latest_date
            );
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
    // --- STEP 1: J-Quants Zone (Bulk update) ---
    // 無料プランのJ-Quantsは株価が12週間遅延するため、直近分はYahooで補う。
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
        // J-Quantsを使わない構成では、Yahoo側に全対象期間を委ねる。
        start_date
    } else if current_date > jquants_end_date {
        current_date
    } else {
        jquants_end_date
    };

    if yahoo_start_date <= today {
        let mut codes = get_unique_codes(&parquet_path)?;
        if codes.is_empty() {
            anyhow::bail!("Yahoo同期対象の銘柄コードがParquetに存在しません");
        }

        if let Some(ref range_str) = args.range {
            let (start, end) = range_str.split_once('-').ok_or_else(|| {
                anyhow::anyhow!("範囲指定の形式が不正です: {range_str}（例: 1000-3000）")
            })?;
            let start: u32 = start.parse()?;
            let end: u32 = end.parse()?;
            if start > end {
                anyhow::bail!("範囲指定の開始値が終了値を超えています: {range_str}");
            }

            codes.retain(|code| {
                code.chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .get(..4)
                    .and_then(|value| value.parse::<u32>().ok())
                    .is_some_and(|value| value >= start && value <= end)
            });
            println!("🎯 [Range Filter] {range_str} (対象: {} 銘柄)", codes.len());
        }

        // ギャップが許容範囲内（前営業日までデータが埋まっている）か判定
        let is_up_to_date_pre_day = match today.weekday().number_from_monday() {
            1 => (today - last_date).num_days() <= 3, // 月曜日の場合、金曜（3日前）まであればOK
            _ => (today - last_date).num_days() <= 1, // 火〜日曜の場合、前日まであればOK
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

            let expected_codes: HashSet<String> = codes
                .iter()
                .map(|code| code.chars().take(4).collect())
                .collect();
            let mut received_codes = HashSet::new();
            let mut bulk_failures = Vec::new();

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

                match fetch_yahoo_bulk(&client, &symbols).await {
                    Ok(results) if results.is_empty() => {
                        bulk_failures.push(format!(
                            "{}〜{}: 応答が空です",
                            symbols.first().unwrap(),
                            symbols.last().unwrap()
                        ));
                    }
                    Ok(results) => {
                        for (code, price, volume) in results {
                            received_codes.insert(code.clone());
                            all_new_rows.push((
                                target_date.to_string(),
                                code,
                                price,
                                price * volume,
                                volume,
                            ));
                        }
                    }
                    Err(error) => {
                        bulk_failures.push(format!(
                            "{}〜{}: {error}",
                            symbols.first().unwrap(),
                            symbols.last().unwrap()
                        ));
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }

            let missing_count = expected_codes.difference(&received_codes).count();
            if !bulk_failures.is_empty() || missing_count > 0 {
                anyhow::bail!(
                    "Yahoo一括取得が不完全です（失敗チャンク: {} / 未取得銘柄: {}）。Parquetは更新しません。詳細: {}",
                    bulk_failures.len(),
                    missing_count,
                    bulk_failures.into_iter().take(3).collect::<Vec<_>>().join(" | ")
                );
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
            let existing_lf = LazyFrame::scan_parquet(&parquet_path, Default::default())?.select([
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

        let file = fs::File::create(&parquet_path)?;
        ParquetWriter::new(file).finish(&mut final_df)?;
        println!(
            "✅ Parquet updated successfully. Total rows: {}",
            final_df.height()
        );

        // Google Drive への書き戻しはワークフローの upload_drive ステップで行う。
        // ここで認証を行わないため、ローカル用 sync_yahoo と認証方式を分離できる。
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

fn parquet_path(range: Option<&str>) -> String {
    match range {
        Some(range) => format!("data/processed_market_data_{range}.parquet"),
        None => "data/processed_market_data.parquet".to_owned(),
    }
}

fn jst_now() -> DateTime<FixedOffset> {
    let jst = FixedOffset::east_opt(9 * 60 * 60).expect("JST offset must be valid");
    Utc::now().with_timezone(&jst)
}

fn latest_required_market_date(now: DateTime<FixedOffset>) -> NaiveDate {
    let today = now.date_naive();
    match today.weekday().number_from_monday() {
        6 => today - Duration::days(1), // 土曜は金曜終値まで
        7 => today - Duration::days(2), // 日曜は金曜終値まで
        _ if now.hour() > 15 || (now.hour() == 15 && now.minute() >= 30) => today,
        _ => previous_weekday(today),
    }
}

fn previous_weekday(mut date: NaiveDate) -> NaiveDate {
    loop {
        date -= Duration::days(1);
        if date.weekday().number_from_monday() <= 5 {
            return date;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jst_datetime(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<FixedOffset> {
        FixedOffset::east_opt(9 * 60 * 60)
            .unwrap()
            .with_ymd_and_hms(year, month, day, hour, minute, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn requires_same_day_data_after_market_close() {
        let friday_night = jst_datetime(2026, 8, 28, 23, 0);
        assert_eq!(
            latest_required_market_date(friday_night),
            NaiveDate::from_ymd_opt(2026, 8, 28).unwrap()
        );
    }

    #[test]
    fn accepts_previous_business_day_before_market_close_or_weekend() {
        let monday_morning = jst_datetime(2026, 8, 31, 7, 0);
        let saturday = jst_datetime(2026, 8, 29, 0, 0);
        let expected = NaiveDate::from_ymd_opt(2026, 8, 28).unwrap();
        assert_eq!(latest_required_market_date(monday_morning), expected);
        assert_eq!(latest_required_market_date(saturday), expected);
    }
}
