use polars::prelude::*;
use chrono::{Local, NaiveDate, Duration, Datelike, Utc, TimeZone};
use jp_stock_system::api::jquants::fetch_daily_bars;
use jp_stock_system::api::yahoo::fetch_ohlc;
use jp_stock_system::alpha::{alpha_a, alpha_b};
use jp_stock_system::utils::settings::Settings;
use std::fs;
use std::path::Path;
use clap::Parser;

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

    if Path::new(PARQUET_PATH).exists() {
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
    
    // デイリーモードなら、前日〜今日分だけをターゲットにする
    let start_date = if args.maintenance {
        last_date.succ_opt().unwrap_or(last_date)
    } else {
        today - Duration::days(2) // 念の為2日前から
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
    // デイリーモードでも、J-Quantsから取れる分（85日前まで）は一括で取得
    let jquants_end_date = today - Duration::days(85);
    let mut current_date = start_date;

    if current_date < jquants_end_date {
        println!("📊 Phase 1: Syncing up to {} using J-Quants Bulk API...", jquants_end_date);
        
        while current_date < jquants_end_date {
            if current_date.weekday().number_from_monday() > 5 {
                current_date += Duration::days(1);
                continue;
            }

            println!("🚀 Fetching bulk data for {} from J-Quants...", current_date);
            match fetch_daily_bars(&client, api_key, &current_date).await {
                Ok(bars) => {
                    if !bars.is_empty() {
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
                Err(e) => eprintln!("❌ Error fetching {}: {}", current_date, e),
            }
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            current_date += Duration::days(1);
        }
    }

    // --- STEP 2: Yahoo Zone (Direct or Bulk) ---
    // 85日前〜今日までの不足分
    let yahoo_start_date = if current_date > jquants_end_date { current_date } else { jquants_end_date };
    
    if yahoo_start_date <= today {
        let codes = get_unique_codes(PARQUET_PATH)?;
        
        if args.maintenance {
            // 🧹 メンテナンスモード: 1件ずつ丁寧に履歴を取得（重い）
            println!("🧹 Phase 2: Running full maintenance sync for {} codes...", codes.len());
            let yahoo_start_ts = Utc.from_utc_datetime(&yahoo_start_date.and_hms_opt(0, 0, 0).unwrap()).timestamp();
            
            for code in codes {
                let symbol = if code.len() == 4 { format!("{}.T", code) } else { format!("{}.T", &code[..4]) };
                let ohlcs = fetch_ohlc(&client, &symbol, yahoo_start_ts).await;
                for ohlc in ohlcs {
                    let d = Utc.timestamp_opt(ohlc.timestamp, 0).unwrap().naive_utc().date();
                    if d >= yahoo_start_date {
                        all_new_rows.push((d.to_string(), code.clone(), ohlc.close, ohlc.close * ohlc.volume, ohlc.volume));
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        } else {
            // 🚀 デイリーモード: 100件ずつ一括で最新値を取得 (超高速)
            println!("🚀 Phase 2: Running lightweight bulk sync for today's prices...");
            
            for chunk in codes.chunks(100) {
                let symbols: Vec<String> = chunk.iter().map(|c| {
                    if c.len() == 4 { format!("{}.T", c) } else { format!("{}.T", &c[..4]) }
                }).collect();
                
                if let Ok(results) = fetch_yahoo_bulk(&client, &symbols).await {
                    for (code, price, volume) in results {
                        all_new_rows.push((today.to_string(), code, price, price * volume, volume));
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
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

        let combined_lf = if Path::new(PARQUET_PATH).exists() {
            let existing_lf = LazyFrame::scan_parquet(PARQUET_PATH, Default::default())?
                .select([col("Date"), col("Code"), col("AdjC"), col("Va"), col("AdjVo"), col("news_text")]);
            concat([existing_lf, new_lf], UnionArgs::default())?
        } else {
            new_lf
        };

        let final_df = combined_lf
            .unique(Some(vec!["Date".into(), "Code".into()]), UniqueKeepStrategy::Last)
            .sort(["Code", "Date"], SortMultipleOptions::default())
            .collect()?;

        // アルファ計算
        println!("🧪 Computing Alphas...");
        let alpha_df = alpha_a::compute(final_df.clone().lazy());
        let alpha_df = alpha_b::compute(alpha_df);
        let mut final_df = alpha_df.collect()?;

        let file = fs::File::create(PARQUET_PATH)?;
        ParquetWriter::new(file).finish(&mut final_df)?;
        println!("✅ Parquet updated. Total rows: {}", final_df.height());
    }

    Ok(())
}

async fn fetch_yahoo_bulk(client: &reqwest::Client, symbols: &[String]) -> anyhow::Result<Vec<(String, f64, f64)>> {
    let url = format!("https://query1.finance.yahoo.com/v7/finance/quote?symbols={}", symbols.join(","));
    let res = client.get(url).send().await?.json::<serde_json::Value>().await?;
    
    let mut results = Vec::new();
    if let Some(quotes) = res["quoteResponse"]["result"].as_array() {
        for quote in quotes {
            let symbol = quote["symbol"].as_str().unwrap_or("");
            let mut code = symbol.replace(".T", "");
            
            // J-Quantsの5桁形式（末尾0）に合わせる
            if code.len() == 4 {
                code.push('0');
            }

            let price = quote["regularMarketPrice"].as_f64().unwrap_or(0.0);
            let volume = quote["regularMarketVolume"].as_f64().unwrap_or(0.0);
            if !code.is_empty() && price > 0.0 {
                results.push((code, price, volume));
            }
        }
    }
    Ok(results)
}

fn get_unique_codes(path: &str) -> anyhow::Result<Vec<String>> {
    if Path::new(path).exists() {
        let df = LazyFrame::scan_parquet(path, Default::default())?
            .select([col("Code")])
            .unique(None, UniqueKeepStrategy::First)
            .collect()?;
        Ok(df.column("Code")?.str()?.into_iter().flatten().map(|s| s.to_string()).collect())
    } else {
        Ok(vec![])
    }
}

