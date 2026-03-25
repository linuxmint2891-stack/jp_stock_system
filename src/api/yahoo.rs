use reqwest;
use csv::ReaderBuilder;

pub async fn fetch_ohlc(symbol: &str) -> Vec<OHLC> {
    let period1 = 1420070400; // 2015-01-01
    let period2 = 1735689600; // 2024-12-31

    let url = format!(
        "https://query1.finance.yahoo.com/v7/finance/download/{}?period1={}&period2={}&interval=1d&events=history",
        symbol, period1, period2
    );

    let resp = reqwest::get(&url).await.unwrap().text().await.unwrap();

    let mut rdr = ReaderBuilder::new()
        .has_headers(true)
        .from_reader(resp.as_bytes());

    let mut data = vec![];

    for result in rdr.records() {
        let record = result.unwrap();

        // Yahoo CSV format:
        // Date,Open,High,Low,Close,Adj Close,Volume

        let open: f64 = record[1].parse().unwrap_or(0.0);
        let high: f64 = record[2].parse().unwrap_or(0.0);
        let low: f64  = record[3].parse().unwrap_or(0.0);
        if &record[4] == "null" {
    continue;
}
        let volume: f64 = record[6].parse().unwrap_or(0.0);

        data.push(OHLC {
            timestamp: data.len(),
            open,
            high,
            low,
            close,
            volume,
        });
    }

    data
}