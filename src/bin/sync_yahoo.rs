use polars::prelude::*;
use chrono::{Local, NaiveDate, Duration, Datelike, Utc, TimeZone};
use jp_stock_system::api::jquants::fetch_daily_bars;
use jp_stock_system::api::yahoo::fetch_ohlc;
use jp_stock_system::alpha::{alpha_a, alpha_b};
use jp_stock_system::utils::settings::Settings;
use jp_stock_system::notifier::send_discord_notification;
use std::fs;
use std::path::Path;

const PARQUET_PATH: &str = "data/processed_market_data.parquet";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let settings = Settings::new()?;
    let api_key = &settings.jquants.api_key;
    let discord_url = std::env::var("DISCORD_WEBHOOK_URL").ok();

    println!("🚀 Starting Hybrid Incremental Data Sync (J-Quants + Yahoo Finance)...");

    // 1. 既存の Parquet から最新日付を取得
    let mut last_date = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();

    if Path::new(PARQUET_PATH).exists() {
        let df_last = LazyFrame::scan_parquet(PARQUET_PATH, Default::default())?
            .select([col("Date").max()])
            .collect()?;
        
        if let Some(date_val) = df_last.column("Date")?.get(0)?.get_str() {
            last_date = NaiveDate::parse_from_str(date_val, "%Y-%m-%d")?;
            println!("📅 Last date in Parquet: {}", last_date);
        }
    }

    // 2. 同期範囲の決定
    let today = Local::now().naive_local().date();
    let jquants_delay_days = 85;
    let boundary_date = today - Duration::days(jquants_delay_days);
    
    let mut current_date = if last_date < NaiveDate::from_ymd_opt(2024, 1, 1).unwrap() {
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()
    } else {
        last_date.succ_opt().unwrap_or(last_date)
    };
    
    if current_date > today {
        println!("✨ No new data to update (Last date is {}).", last_date);
        return Ok(());
    }

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .build()?;
    let mut all_new_rows = Vec::new();

    // --- STEP 1: J-Quants Zone (Older than boundary) ---
    if current_date < boundary_date {
        println!("📊 Phase 1: Syncing from {} to {} using J-Quants...", current_date, boundary_date.pred_opt().unwrap_or(boundary_date));
        
        while current_date < boundary_date {
            if current_date.weekday().number_from_monday() > 5 {
                current_date += Duration::days(1);
                continue;
            }

            println!("🚀 Fetching data for {} from J-Quants...", current_date);
            match fetch_daily_bars(&client, api_key, &current_date).await {
                Ok(bars) => {
                    if bars.is_empty() {
                        println!("ℹ️ No data for {} (Market closed or not yet available).", current_date);
                    } else {
                        println!("✅ Received {} quotes.", bars.len());
                        for bar in bars {
                            let code = bar["Code"].as_str().unwrap_or("").to_string();
                            let date = bar["Date"].as_str().unwrap_or("").to_string();
                            let close = bar["AdjustmentClose"].as_f64()
                                .or_else(|| bar["AdjC"].as_f64())
                                .unwrap_or(0.0);
                            let volume = bar["AdjustmentVolume"].as_f64()
                                .or_else(|| bar["AdjVo"].as_f64())
                                .unwrap_or(0.0);
                            let turnover = bar["TurnoverValue"].as_f64()
                                .or_else(|| bar["Va"].as_f64())
                                .unwrap_or(0.0);

                            if !code.is_empty() {
                                all_new_rows.push((date, code, close, turnover, volume));
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("❌ Error fetching {}: {}", current_date, e);
                    if e.to_string().contains("429") {
                        println!("🛑 Rate limit hit (429). Waiting 60 seconds...");
                        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                        continue; 
                    }
                    if e.to_string().contains("401") || e.to_string().contains("Unauthorized") {
                        println!("🚨 APIキーエラーが発生しました。");
                        return Err(e);
                    }
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(13000)).await;
            current_date += Duration::days(1);
        }
    }

    // --- STEP 2: Yahoo Zone (Last 3 months) ---
    if current_date <= today {
        println!("📊 Phase 2: Syncing from {} to {} using Yahoo Finance...", current_date, today);
        
        let codes = get_unique_codes(PARQUET_PATH)?;
        if codes.is_empty() {
            println!("⚠️ No stock codes found. Skipping Yahoo sync.");
        } else {
            println!("🔍 Fetching data for {} codes from Yahoo Finance...", codes.len());
            
            let start_date_for_yahoo = current_date - Duration::days(1);
            let yahoo_start_ts = Utc.from_utc_datetime(&start_date_for_yahoo.and_hms_opt(0, 0, 0).unwrap()).timestamp();
            
            let mut consecutive_failures = 0;
            let mut count = 0;
            let total_codes = codes.len();

            for code in codes {
                count += 1;
                if count % 10 == 0 {
                    println!("⏳ Progress: {}/{} codes processed", count, total_codes);
                }

                let symbol = if code.ends_with('0') && code.len() > 4 {
                    format!("{}.T", &code[..code.len() - 1])
                } else {
                    format!("{}.T", code)
                };

                let ohlcs = fetch_ohlc(&client, &symbol, yahoo_start_ts).await;
                
                if ohlcs.is_empty() {
                    consecutive_failures += 1;
                    if consecutive_failures >= 5 {
                        println!("🛑 Consecutive failures detected. Yahoo might be blocking our IP.");
                        println!("🛑 Sleeping for 10 minutes to cool down...");
                        tokio::time::sleep(tokio::time::Duration::from_secs(600)).await;
                        consecutive_failures = 0;
                    }
                } else {
                    consecutive_failures = 0;
                    for ohlc in ohlcs {
                        let d = Utc.timestamp_opt(ohlc.timestamp, 0).unwrap().naive_utc().date();
                        if d >= current_date && d <= today {
                            all_new_rows.push((d.to_string(), code.clone(), ohlc.close, ohlc.close * ohlc.volume, ohlc.volume));
                        }
                    }
                }
            }
            println!("✅ Yahoo Finance sync completed.");
        }
    }

    // 3. データの結合と保存
    if !all_new_rows.is_empty() {
        println!("🔗 Merging {} new rows with existing data...", all_new_rows.len());
        
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

        let combined_lf = if Path::new(PARQUET_PATH).exists() {
            let existing_lf = LazyFrame::scan_parquet(PARQUET_PATH, Default::default())?
                .select([
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

        let combined_lf = combined_lf
            .unique(Some(vec!["Date".into(), "Code".into()]), UniqueKeepStrategy::Last)
            .sort(["Code", "Date"], SortMultipleOptions::default());

        println!("🧪 Computing Alphas...");
        let final_lf = alpha_a::compute(combined_lf);
        let final_lf = alpha_b::compute(final_lf);

        let mut final_df = final_lf.collect()?;

        let file = fs::File::create(PARQUET_PATH)?;
        ParquetWriter::new(file).finish(&mut final_df)?;

        println!("✅ Parquet updated. Total rows: {}", final_df.height());

        if let Some(url) = discord_url {
            let _ = send_discord_notification(
                &url,
                "Hybrid Sync Success",
                "Incremental Update",
                0.0,
                1.0,
                &format!("Updated Parquet: Total rows {}", final_df.height()),
                "None"
            ).await;
        }
    } else {
        println!("✨ No new rows were added.");
    }

    Ok(())
}

fn get_unique_codes(path: &str) -> anyhow::Result<Vec<String>> {
    if Path::new(path).exists() {
        let df = LazyFrame::scan_parquet(path, Default::default())?
            .select([col("Code")])
            .unique(None, UniqueKeepStrategy::First)
            .collect()?;
        let codes = df.column("Code")?.str()?
            .into_iter()
            .flatten()
            .map(|s| s.to_string())
            .collect();
        Ok(codes)
    } else {
        println!("ℹ️ Parquet not found, reading from jpx_codes.csv...");
        let df = LazyCsvReader::new("jpx_codes.csv")
            .finish()?
            .collect()?;
        
        let mask = df.column("市場・商品区分")?.str()?
            .into_iter()
            .map(|s| s != Some("PRO Market") && s != Some("市場・商品区分"))
            .collect::<BooleanChunked>();
        
        let filtered_df = df.filter(&mask)?;

        let codes = filtered_df.column("コード")?.cast(&DataType::String)?
            .str()?
            .into_iter()
            .flatten()
            .map(|s: &str| {
                if s.len() == 4 {
                    format!("{}0", s)
                } else {
                    s.to_string()
                }
            })
            .collect();
        Ok(codes)
    }
}
