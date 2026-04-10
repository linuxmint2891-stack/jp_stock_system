use reqwest::blocking::Client;
use std::fs::File;
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    
    // 1. 画像にある「x-api-key」を使用
    let api_key = "-fMC9EnlXau-2iA_I3xk6cyZxAI_xZutVBNVeht3VsU"; // お持ちのキー

    println!("Fetching daily prices via J-Quants API V2...");

    // 2. V2の価格取得エンドポイント
    // ※無料プランで取得可能な範囲を指定するためには ?date=20240104 などを付与してください
    let res = client
        .get("https://api.jquants.com/v2/equities/bars/daily?date=20240329") // サンプルとして直近の営業日を入れています
        .header("x-api-key", api_key)
        .send()?;

    let status = res.status();
    let body_text = res.text()?;

    if status.is_success() {
        println!("✅ Success! Data received.");
        
        // 3. 取得したデータをCSVとして保存
        let mut file = File::create("data/all_stocks_daily.csv")?;
        file.write_all(body_text.as_bytes())?;
        
        println!("Saved to data/all_stocks_daily.csv");
        println!("Next step: run 'cargo run --bin sync_data'");
    } else {
        println!("❌ API Error: {}", status);
        println!("Message: {}", body_text);
    }

    Ok(())
}