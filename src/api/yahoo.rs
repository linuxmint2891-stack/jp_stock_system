use tokio::fs::File;
use std::error::Error;
use crate::model::ohlc::OHLC;
use reqwest::Client;

use tokio::io::BufReader;
use tokio::io::AsyncBufReadExt;

use rand::Rng;

pub async fn fetch_ohlc(code: &str) -> Vec<OHLC> {
    let mut rng = rand::thread_rng();

    let mut price: f64 = 100.0;
    let mut data = Vec::new();

    for i in 0..30 {
        let change = rng.gen_range(-0.05..0.05);

        let open = price;
        let close = price * (1.0 + change);

        let high = open.max(close) + rng.gen_range(0.0..2.0);
        let low = open.min(close) - rng.gen_range(0.0..2.0);

        data.push(OHLC {
            open,
            high,
            low,
            close,
            timestamp: i, // 仮
            volume: rng.gen_range(1000.0..10000.0),
        });

        price = close;
    }

    data
}

async fn load_codes() -> Result<Vec<String>, Box<dyn Error>> {
    let file = File::open("/home/michiharu/my_project/jp_stock_system/data/jpx_codes.csv").await?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    
    let mut codes = Vec::new();
    
    // ヘッダー行をスキップ
    if let Some(_) = lines.next_line().await? {}

    // データ行を読み込む
    while let Some(line_result) = lines.next_line().await.unwrap() {
    let line: String = line_result;

    let code = line.split(',').next().unwrap_or("").trim();

    println!("{}", code);
}
    
    Ok(codes)
}

#[tokio::main]
async fn main() {
    match load_codes().await {
        Ok(codes) => println!("銘柄コード取得成功: {:?}, 件数: {}", codes, codes.len()),
        Err(e) => eprintln!("エラー: {}", e),
    }
}