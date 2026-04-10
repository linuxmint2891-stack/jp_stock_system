use polars::prelude::*;

pub fn compute(returns: &DataFrame, window: usize) -> PolarsResult<DataFrame> {
    let stock_names: Vec<String> = returns
        .get_columns()
        .iter()
        .filter(|s| s.dtype().is_numeric())
        .map(|s| s.name().to_string())
        .collect();

    let mut lf = returns.clone().lazy();

    // ❌ NG: 4000回のループで計算グラフを深くする
    // ✅ OK: 1回の with_columns に 4000個の Expression を渡す
    let expressions: Vec<Expr> = stock_names.iter().map(|name| {
        ((col(name) - col(name).rolling_mean(RollingOptionsFixedWindow {
            window_size: window,
            min_periods: window,
            ..Default::default()
        })) * lit(-1.0)).alias(name)
    }).collect();

    // これでスタック消費を劇的に抑えられます
    lf = lf.with_columns(expressions);
    
    // ... その後の正規化も同様に with_columns([ ... ]) を使って一括処理
    lf.collect()
}
 