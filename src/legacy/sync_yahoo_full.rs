use polars::prelude::*;
use chrono::{Local, NaiveDate, Duration, Datelike, Utc, TimeZone};
use jp_stock_system::api::yahoo::fetch_ohlc;
use jp_stock_system::alpha::{alpha_a, alpha_b};
use jp_stock_system::notifier::send_discord_notification;
use std::fs;
use std::path::Path;

const PARQUET_PATH: &str = "data/processed_market_data.parquet";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let discord_url = std::env::var("DISCORD_WEBHOOK_URL").ok();

    println!("🚀 Starting FULL Yahoo Finance Data Sync...");
    println!("⚠️ This will fetch historical data for ALL Japanese stocks. It may take several hours.");

    // 1. 銘柄一覧を取得
    let codes = get_all_codes()?;
    println!("🔍 Total codes to fetch: {}", codes.len());

    // 2. 取得範囲の決定 (過去2年分)
    let today = Local::now().naive_local().date();
    let start_date = today - Duration::days(730);
    let start_ts = Utc.from_utc_datetime(&start_date.and_hms_opt(0, 0, 0).unwrap()).timestamp();

    println!("📅 Period: {} to {}", start_date, today);

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .build()?;
    let mut all_new_rows = Vec::new();

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

        let ohlcs = fetch_ohlc(&client, &symbol, start_ts).await;
        
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
                let date_str = d.to_string();
                all_new_rows.push((date_str, code.clone(), ohlc.close, ohlc.close * ohlc.volume, ohlc.volume));
            }
        }

        // 大量データによるメモリ圧迫を避けるため、一定件数ごとに保存
        if all_new_rows.len() > 50_000 {
            save_incremental(&all_new_rows)?;
            all_new_rows.clear();
        }
    }

    if !all_new_rows.is_empty() {
        save_incremental(&all_new_rows)?;
    }

    println!("✅ FULL Sync completed.");

    if let Some(url) = discord_url {
        let _ = send_discord_notification(
            &url,
            "Yahoo Full Sync Success",
            "Initial Data Collection",
            0.0,
            1.0,
            "Yahoo Finance full sync has completed.",
            "None"
        ).await;
    }

    Ok(())
}

fn get_all_codes() -> anyhow::Result<Vec<String>> {
    println!("ℹ️ Reading codes from jpx_codes.csv...");
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
        .map(|s| {
            if s.len() == 4 {
                format!("{}0", s)
            } else {
                s.to_string()
            }
        })
        .collect();
    Ok(codes)
}

fn save_incremental(rows: &[(String, String, f64, f64, f64)]) -> anyhow::Result<()> {
    if rows.is_empty() { return Ok(()); }

    println!("💾 Saving {} rows to Parquet...", rows.len());

    let dates: Vec<String> = rows.iter().map(|x| x.0.clone()).collect();
    let codes: Vec<String> = rows.iter().map(|x| x.1.clone()).collect();
    let adj_c: Vec<f64> = rows.iter().map(|x| x.2).collect();
    let va: Vec<f64> = rows.iter().map(|x| x.3).collect();
    let adj_vo: Vec<f64> = rows.iter().map(|x| x.4).collect();

    let new_lf = df!(
        "Date" => dates,
        "Code" => codes,
        "AdjC" => adj_c,
        "Va" => va,
        "AdjVo" => adj_vo
    )?.lazy().with_column(lit("").alias("news_text"));

    let combined_lf = if Path::new(PARQUET_PATH).exists() {
        let existing_lf = LazyFrame::scan_parquet(PARQUET_PATH, Default::default())?;
        concat([existing_lf, new_lf], UnionArgs::default())?
    } else {
        new_lf
    };

    let combined_lf = combined_lf
        .unique(Some(vec!["Date".into(), "Code".into()]), UniqueKeepStrategy::Last)
        .sort(["Code", "Date"], SortMultipleOptions::default());

    let final_lf = alpha_a::compute(combined_lf);
    let final_lf = alpha_b::compute(final_lf);

    let mut final_df = final_lf.collect()?;
    let file = fs::File::create(PARQUET_PATH)?;
    ParquetWriter::new(file).finish(&mut final_df)?;

    Ok(())
}
