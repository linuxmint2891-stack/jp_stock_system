use jp_stock_system::api::yahoo::fetch_ohlc;
use reqwest::Client;
use chrono::{Utc, Duration, TimeZone};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::new();
    let symbols = vec!["1301.T", "7203.T", "3810.T"];
    
    // 過去30日分のデータを取得してみる
    let start_date = Utc::now().naive_utc().date() - Duration::days(30);
    let start_ts = Utc.from_utc_datetime(&start_date.and_hms_opt(0, 0, 0).unwrap()).timestamp();

    println!("🧪 Testing Yahoo Japan Scraping Logic...");
    println!("📅 Target Start Date: {} (TS: {})", start_date, start_ts);

    for symbol in symbols {
        println!("\n--- Fetching: {} ---", symbol);
        let data = fetch_ohlc(&client, symbol, start_ts).await;
        
        if data.is_empty() {
            println!("❌ No data fetched for {}", symbol);
        } else {
            println!("✅ Successfully fetched {} rows", data.len());
            for ohlc in data.iter().take(3) {
                println!("  Date TS: {}, Close: {}, Volume: {}", ohlc.timestamp, ohlc.close, ohlc.volume);
            }
        }
    }

    Ok(())
}
