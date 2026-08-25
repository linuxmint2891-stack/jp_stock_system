use crate::model::ohlc::OHLC;
use anyhow::{bail, Result};
use chrono::{NaiveDate, TimeZone, Utc};
use futures::stream::{self, StreamExt};
use regex::Regex;
use reqwest::Client;
use std::time::Duration;

/// Yahoo FinanceのQuote APIから複数銘柄の現在価格と出来高を取得する。
/// 戻り値の銘柄コードは、呼び出し側のParquetと揃う4桁コードに正規化する。
pub async fn fetch_yahoo_bulk(
    client: &Client,
    symbols: &[String],
) -> Result<Vec<(String, f64, f64)>> {
    if symbols.is_empty() {
        return Ok(Vec::new());
    }

    // v7/finance/quote は2026年時点で認証必須になっている。
    // 認証不要の Chart API を銘柄ごとに呼び、並列数を制限して負荷を抑える。
    const MAX_CONCURRENT_REQUESTS: usize = 8;
    let outcomes = stream::iter(symbols.iter().cloned())
        .map(|symbol| async move { fetch_yahoo_chart_quote(client, &symbol).await })
        .buffer_unordered(MAX_CONCURRENT_REQUESTS)
        .collect::<Vec<_>>()
        .await;

    outcomes.into_iter().collect()
}

async fn fetch_yahoo_chart_quote(client: &Client, symbol: &str) -> Result<(String, f64, f64)> {
    let url = format!("https://query1.finance.yahoo.com/v8/finance/chart/{symbol}");
    const MAX_RETRIES: u32 = 3;
    for attempt in 1..=MAX_RETRIES {
        match client
            .get(&url)
            .query(&[("range", "5d"), ("interval", "1d")])
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                let body: serde_json::Value = response.json().await?;
                let chart = body["chart"]["result"]
                    .as_array()
                    .and_then(|results| results.first())
                    .ok_or_else(|| anyhow::anyhow!("Yahoo chart response has no result: {body}"))?;
                let meta = &chart["meta"];
                let returned_symbol = meta["symbol"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Yahoo chart response has no symbol: {body}"))?;
                let price = meta["regularMarketPrice"].as_f64().ok_or_else(|| {
                    anyhow::anyhow!(
                        "Yahoo chart response has no market price for {returned_symbol}"
                    )
                })?;
                let volume = meta["regularMarketVolume"].as_f64().ok_or_else(|| {
                    anyhow::anyhow!(
                        "Yahoo chart response has no market volume for {returned_symbol}"
                    )
                })?;
                let code = returned_symbol
                    .strip_suffix(".T")
                    .unwrap_or(returned_symbol)
                    .to_string();
                return Ok((code, price, volume));
            }
            Ok(response) => {
                let status = response.status();
                let detail = response.text().await.unwrap_or_default();
                if attempt == MAX_RETRIES
                    || !(status.is_server_error()
                        || status == reqwest::StatusCode::TOO_MANY_REQUESTS)
                {
                    bail!(
                        "Yahoo chart request failed for {symbol} with status {status}: {}",
                        detail.chars().take(300).collect::<String>()
                    );
                }
                eprintln!(
                    "⚠️ Yahoo chart request failed ({symbol}: {status}, retry {attempt}/{MAX_RETRIES})"
                );
            }
            Err(error) => {
                if attempt == MAX_RETRIES {
                    return Err(error.into());
                }
                eprintln!(
                    "⚠️ Yahoo chart connection failed ({symbol}: {error}, retry {attempt}/{MAX_RETRIES})"
                );
            }
        }
        tokio::time::sleep(Duration::from_secs(attempt as u64)).await;
    }

    unreachable!("retry loop always returns")
}

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
