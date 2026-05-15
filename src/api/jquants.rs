use reqwest::Client;
use serde_json::Value;
use chrono::NaiveDate;
use anyhow::Result;

pub async fn fetch_daily_bars(client: &Client, api_key: &str, date: &NaiveDate) -> Result<Vec<Value>> {
    let url = "https://api.jquants.com/v2/equities/bars/daily";

    // Try YYYY-MM-DD first as seen in get_prices.rs
    let date_query = date.format("%Y-%m-%d").to_string();

    let res = client.get(url)
        .header("x-api-key", api_key)
        .query(&[("date", &date_query)])
        .send()
        .await?;

    if res.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(anyhow::anyhow!("429 Too Many Requests"));
    }

    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("J-Quants API Error: Status {} - {}", status, text));
    }

    let body: Value = res.json().await?;
    let data_node = body["daily_bars"].as_array()
        .or_else(|| body["data"].as_array())
        .or_else(|| body["daily_quotes"].as_array());
    
    if let Some(bars) = data_node {
        Ok(bars.clone())
    } else {
        Ok(vec![])
    }
}
