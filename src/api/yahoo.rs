use crate::model::ohlc::OHLC;

pub async fn fetch_ohlc(symbol: &str) -> Vec<OHLC> {

    let url = format!(
    "https://query1.finance.yahoo.com/v8/finance/chart/{}?range=5y&interval=1d",
    symbol
);

    // =========================
    // ✅ ここから処理（関数の中）
    // =========================
    let client = reqwest::Client::new();

let resp = match client
    .get(&url)
    .header("User-Agent", "Mozilla/5.0")
    .send()
    .await
{
    Ok(r) => r,
    Err(_) => return vec![],
};

    if !resp.status().is_success() {
        return vec![];
    }

    let text = match resp.text().await {
        Ok(t) => t,
        Err(_) => return vec![],
    };

    let json: serde_json::Value = match serde_json::from_str(&text) {
        Ok(j) => j,
        Err(_) => {
            println!("⚠️ 非JSON: {}", symbol);
            return vec![];
        }
    };

    // =========================
    // JSONパース
    // =========================
    let result = &json["chart"]["result"];
    if result.is_null() {
        return vec![];
    }

    let result = &result[0];

    let timestamps = match result["timestamp"].as_array() {
        Some(v) => v,
        None => return vec![],
    };

    let quote = &result["indicators"]["quote"][0];

    let opens = match quote["open"].as_array() {
        Some(v) => v,
        None => return vec![],
    };

    let highs = match quote["high"].as_array() {
        Some(v) => v,
        None => return vec![],
    };

    let lows = match quote["low"].as_array() {
        Some(v) => v,
        None => return vec![],
    };

    let closes = match quote["close"].as_array() {
        Some(v) => v,
        None => return vec![],
    };

    let volumes = match quote["volume"].as_array() {
        Some(v) => v,
        None => return vec![],
    };

    let mut data = Vec::new();

let len = [
    timestamps.len(),
    opens.len(),
    highs.len(),
    lows.len(),
    closes.len(),
    volumes.len(),
].into_iter().min().unwrap();

for i in 0..len {

    let timestamp = match timestamps[i].as_i64() {
        Some(v) => v,
        None => continue,
    };

    let open = match opens[i].as_f64() {
        Some(v) => v,
        None => continue,
    };

    let high = match highs[i].as_f64() {
        Some(v) => v,
        None => continue,
    };

    let low = match lows[i].as_f64() {
        Some(v) => v,
        None => continue,
    };

    let close = match closes[i].as_f64() {
        Some(v) => v,
        None => continue,
    };

    let volume = volumes[i].as_f64().unwrap_or(0.0);

    data.push(OHLC {
        timestamp,
        open,
        high,
        low,
        close,
        volume,
    });
}

    data
}