pub fn trend_regime(market_returns: &[f64]) -> bool {
    if market_returns.len() < 10 {
        return false;
    }

    // 累積リターン（トレンドの強さ）
    let cum_return: f64 = market_returns.iter().sum();

    // ボラ
    let vol: f64 = market_returns
        .iter()
        .map(|r| r * r)
        .sum::<f64>()
        .sqrt();

    // トレンド強度（Sharpeっぽい）
    let trend_strength = cum_return / (vol + 1e-6);

    trend_strength > 0.5
}