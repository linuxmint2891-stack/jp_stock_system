use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Response {
    pub chart: Chart,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Chart {
    pub result: Vec<ResultItem>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ResultItem {
    pub meta: Meta,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Meta {

    pub symbol: String,

    #[serde(rename="shortName")]
    pub short_name: String,

    #[serde(rename="regularMarketPrice")]
    pub regular_market_price: f64,

    #[serde(rename="regularMarketVolume")]
    pub regular_market_volume: u64,
}