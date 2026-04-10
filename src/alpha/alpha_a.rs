use polars::prelude::*;

pub fn compute(df: &DataFrame) -> PolarsResult<DataFrame> {
    Ok(df.clone())
}