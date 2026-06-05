use polars::prelude::*;

pub fn compute(lf: LazyFrame) -> LazyFrame {
    lf.with_columns([
        (col("AdjC") / col("AdjC").rolling_mean(RollingOptionsFixedWindow {
            window_size: 20,
            min_periods: 1,
            ..Default::default()
        }).over([col("Code")]) - lit(1.0)).alias("alpha_b")
    ])
}
