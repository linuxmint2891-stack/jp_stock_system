use reqwest::Client;
use crate::model::{Response, Meta};

pub async fn fetch_stock(
    client: &Client,
    code: &str
) -> Option<Meta> {

    let url = format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{}",
        code
    );

    let res = client
        .get(&url)
        .header("User-Agent","Mozilla/5.0")
        .send()
        .await
        .ok()?;

    let data: Response = res.json().await.ok()?;

    Some(data.chart.result.get(0)?.meta.clone())
}