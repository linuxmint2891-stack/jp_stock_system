use reqwest::Client;
use serde_json::{Value, json};
use std::fs::File;
use std::io::Write;
use chrono::{NaiveDate, Duration, Local, Datelike, Weekday};

use std::fs;
use std::path::Path;
use tokio;
use dotenvy::dotenv;
use std::env;

#[tokio::main]
async fn main() {
    dotenv().ok();
    let target_dir = "data"; // JSON保存先
    let min_valid_size = 800 * 1024; // 800KB しきい値

    // APIキーの取得
    let api_key = env::var("JQUANTS_API_KEY").unwrap_or_else(|_| {
        // フォールバックとして既存のキー（auth.rsにあるもの）を使用
        "-fMC9EnlXau-2iA_I3xk6cyZxAI_xZutVBNVeht3VsU".to_string()
    });

    // 1. 保存先ディレクトリの準備
    if let Err(e) = fs::create_dir_all(target_dir) {
        eprintln!("Error creating directory: {}", e);
        return;
    }

    // 2. 取得したい日付リストの作成 (2年前 〜 3ヶ月前)
    let today = Local::now().naive_local().date();
    
    // 開始日: 本日の2年前 (概算 730日)
    let start_date = today - Duration::days(730);
    // 終了日: 本日の3ヶ月前 (概算 90日)
    let end_date = today - Duration::days(90);

    println!("📅 Target range: {} to {}", start_date, end_date);

    let mut target_dates: Vec<NaiveDate> = Vec::new();
    let mut current = start_date;
    while current <= end_date {
        target_dates.push(current);
        current += Duration::days(1);
    }

    let client = Client::new();

    // 3. メインループ
    for date in target_dates {
        let date_str = date.format("%Y-%m-%d").to_string();

        // 3a. 土日は市場が休みなのでリクエストせずにスキップ
        if date.weekday() == Weekday::Sat || date.weekday() == Weekday::Sun {
            continue;
        }

        // ファイル名を daily_YYYY-MM-DD.json に統一
        let file_path = format!("{}/daily_{}.json", target_dir, date_str);
        let path = Path::new(&file_path);

        // --- 改善されたレジューム & 破損チェックロジック ---
        let should_download = match fs::metadata(path) {
            Ok(metadata) => {
                let size = metadata.len();
                
                if size < min_valid_size {
                    // 以前に取得した際に「データなし」として保存した空ファイル（約25バイト前後）かチェック
                    // 1KB未満なら「休場日」として取得済みとみなす
                    if size < 1024 {
                        false
                    } else {
                        println!("⚠️ File {} is small ({} KB). Re-downloading...", date_str, size / 1024);
                        true
                    }
                } else {
                    // 健全なファイルがある場合はスキップ
                    false
                }
            }
            Err(_) => true, // ファイルが存在しない
        };

        if !should_download {
            continue;
        }

        // 4. 実際のダウンロード処理
        println!("🚀 Fetching data for {}...", date_str);
        match fetch_data(&client, &api_key, &date).await {
            Ok(_) => {
                // Freeプランのレートリミット (5req/min) に配慮して約13秒待機
                tokio::time::sleep(tokio::time::Duration::from_millis(13000)).await;
            }
            Err(e) => {
                eprintln!("❌ Error fetching {}: {}", date_str, e);
                if e.to_string().contains("429") {
                    // 大幅な超過による5分間のアカウントブロックを回避するため、305秒待機
                    println!("🛑 Rate limit hit. Waiting for penalty to clear (305s)...");
                    tokio::time::sleep(tokio::time::Duration::from_secs(305)).await;
                }
            }
        }
    }

    println!("🎯 All tasks completed!");
}

async fn fetch_data(client: &Client, api_key: &str, date: &NaiveDate) -> Result<bool, Box<dyn std::error::Error>> {
    let url = "https://api.jquants.com/v2/equities/bars/daily";
    let date_str = date.format("%Y-%m-%d").to_string();
    let file_path = format!("data/daily_{}.json", date_str);

    let res = client.get(url)
        .header("x-api-key", api_key)
        .query(&[("date", &date_str)])
        .send()
        .await?;

    if res.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err("429 Too Many Requests".into());
    }

    if !res.status().is_success() {
        return Err(format!("API Error: Status {}", res.status()).into());
    }

    let body: Value = res.json().await?;
    let data_node = body["daily_bars"].as_array().or_else(|| body["data"].as_array());
    
    if let Some(q) = data_node {
        if !q.is_empty() {
            let mut file = File::create(&file_path)?;
            let json_output = json!({ "daily_quotes": q });
            file.write_all(json_output.to_string().as_bytes())?;
            println!("✅ Saved {} quotes to {}", q.len(), file_path);
            return Ok(true);
        } else {
            // データが空の場合（休場日）もファイルを作成することで、次回の重複リクエストを防ぐ
            let mut file = File::create(&file_path)?;
            let json_output = json!({ "daily_quotes": [] });
            file.write_all(json_output.to_string().as_bytes())?;
            println!("ℹ️ No data for {} (Market closed). Saved empty file.", date_str);
            return Ok(false);
        }
    }
    
    // データフィールド自体がない場合も空ファイルを保存
    let mut file = File::create(&file_path)?;
    let json_output = json!({ "daily_quotes": [] });
    file.write_all(json_output.to_string().as_bytes())?;
    Ok(false)
}