use polars::prelude::*; // これを追加

pub mod alpha_a;
pub mod mean_reversion;

pub enum Strategy {
    AlphaA,
    MeanReversion,
}

pub fn generate_score(df: &DataFrame, strategy: Strategy) -> PolarsResult<DataFrame> {
    match strategy {
        Strategy::AlphaA => alpha_a::compute(df),
        Strategy::MeanReversion => mean_reversion::compute(df, 20),
    }
}