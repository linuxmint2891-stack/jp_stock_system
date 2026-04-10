use reqwest::blocking::Client;
use std::fs::File;
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let api_key = "-fMC9EnlXau-2iA_I3xk6cyZxAI_xZutVBNVeht3VsU";
    let url = "https://api.jquants.com/v2/equities/bars/daily";

    // 無料プランの最終日を狙い撃ち（念のためハイフンありで試します）
    let date = ["2026-01-14""2026-01-15"]; 
    
    println!("Requesting json data for {}...", date);

    let res = client
        .get(url)
        .header("x-api-key", api_key)
        .query(&[("date", date)])
        .send()?;

    let status = res.status();
    let body = res.text()?; // ここで一度中身を全部受け取る

    if status.is_success() {
        if body.len() < 100 {
            println!("⚠️ Warning: Response body is suspicious (too short):");
            println!("{}", body);
        } else {
            // CSVではなく、そのままJSONとして保存する
            let mut file = File::create("data/all_stocks_daily.json")?;
            file.write_all(body.as_bytes())?;
            println!("✅ Success! Saved to data/all_stocks_daily.json (Size: {} bytes)", body.len());
        }
    } else {
        println!("❌ API Error: {}", status);
        println!("Body: {}", body);
    }

    Ok(())
}