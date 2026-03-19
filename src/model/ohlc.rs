// src/model/ohlc.rs

#[derive(Debug, Clone)]
pub struct OHLC {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub timestamp: i64,
    pub volume: f64,
}