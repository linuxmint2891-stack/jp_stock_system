use serde::Deserialize;
use tokio::fs;

#[derive(Debug, Deserialize)]
pub struct Meta {
    pub currency: String,
    pub symbol: String,
    pub exchange_name: String,
    pub instrument_type: String,
    pub first_trade_date: u64,
    pub regular_market_time: u64,
    pub gmtoffset: i64,
    pub timezone: String,
    pub exchange_timezone_name: String,
    pub regular_market_price: f64,
}

// 非同期関数で JSON を読み込み、銘柄コードだけ Vec<String> にする
async fn load_codes() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    // JSON ファイルを非同期で読み込む
    let data = fs::read_to_string("stocks.json").await?;
    
    // JSON を Stock ベクターにデシリアライズ
    let stocks: Vec<Meta> = serde_json::from_str(&data)?;
    
    // code だけ抽出
    let codes = stocks.into_iter().map(|s| s.symbol).collect();
    
    Ok(codes)
}

#[tokio::main]
async fn main() {
    match load_codes().await {
        Ok(codes) => println!("銘柄コード取得成功: {:?}, 件数: {}", codes, codes.len()),
        Err(e) => eprintln!("銘柄コード取得エラー: {}", e),
    }
}