use crate::model::ohlc::OHLC;
use chrono::{NaiveDate, TimeZone, Utc};
use regex::Regex;
use reqwest::Client;
use std::time::Duration;

pub async fn fetch_ohlc(client: &Client, symbol: &str, start_timestamp: i64) -> Vec<OHLC> {
    let clean_symbol = symbol.replace(".T", "");
    let url = format!(
        "https://finance.yahoo.co.jp/quote/{}.T/history",
        clean_symbol
    );
    let referer = format!("https://finance.yahoo.co.jp/quote/{}.T", clean_symbol);

    let mut retry_count = 0;
    let max_retries = 3;
    let mut resp_text = String::new();

    while retry_count < max_retries {
        // 負荷軽減のためのウェイト（デフォルト 2秒 + リトライ時は大幅に増やす）
        let base_wait = if retry_count == 0 {
            2000
        } else {
            60000 * retry_count
        }; // リトライ時は分単位で待機
        tokio::time::sleep(Duration::from_millis(base_wait as u64)).await;

        let res = client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36")
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8")
            .header("Accept-Language", "ja,en-US;q=0.9,en;q=0.8")
            .header("Referer", &referer)
            .header("Connection", "keep-alive")
            .header("Upgrade-Insecure-Requests", "1")
            .timeout(Duration::from_secs(30))
            .send()
            .await;

        match res {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    if let Ok(text) = resp.text().await {
                        if text.contains("_StyledNumber__value") {
                            resp_text = text;
                            break;
                        } else {
                            println!(
                                "⚠️ Yahoo Response ({}): No data found in HTML (Bot detected?)",
                                symbol
                            );
                        }
                    }
                } else if status == reqwest::StatusCode::NOT_FOUND {
                    return vec![];
                } else if status.is_server_error()
                    || status == reqwest::StatusCode::TOO_MANY_REQUESTS
                {
                    println!(
                        "⚠️ Yahoo Error ({}): {} (Retry {}/{})",
                        symbol,
                        status,
                        retry_count + 1,
                        max_retries
                    );
                } else {
                    println!("⚠️ Unexpected Status ({}): {}", symbol, status);
                    return vec![];
                }
            }
            Err(e) => {
                println!(
                    "❌ Connection Error ({}): {} (Retry {}/{})",
                    symbol,
                    e,
                    retry_count + 1,
                    max_retries
                );
            }
        }
        retry_count += 1;
    }

    if resp_text.is_empty() {
        return vec![];
    }

    let tr_re = Regex::new(r#"(?s)<tr[^>]*>(.*?)</tr>"#).unwrap();
    let date_re = Regex::new(r#"<th[^>]*>(\d{4}/\d{1,2}/\d{1,2})</th>"#).unwrap();
    let val_re =
        Regex::new(r#"<span[^>]*class="[^"]*_StyledNumber__value[^"]*"[^>]*>(.*?)</span>"#)
            .unwrap();

    let mut data = Vec::new();
    for tr_cap in tr_re.captures_iter(&resp_text) {
        let tr_content = &tr_cap[1];
        if let Some(date_cap) = date_re.captures(tr_content) {
            let date_str = &date_cap[1];
            let mut vals = Vec::new();
            for val_cap in val_re.captures_iter(tr_content) {
                let val_str = val_cap[1].replace(",", "");
                if let Ok(val) = val_str.parse::<f64>() {
                    vals.push(val);
                }
            }
            if vals.len() >= 5 {
                if let Ok(naive_date) = NaiveDate::parse_from_str(date_str, "%Y/%m/%d") {
                    let timestamp = Utc
                        .from_utc_datetime(&naive_date.and_hms_opt(0, 0, 0).unwrap())
                        .timestamp();
                    if timestamp >= start_timestamp {
                        data.push(OHLC {
                            timestamp,
                            open: vals[0],
                            high: vals[1],
                            low: vals[2],
                            close: vals[3],
                            volume: vals[4],
                        });
                    }
                }
            }
        }
    }
    data.sort_by_key(|d| d.timestamp);
    data
}
