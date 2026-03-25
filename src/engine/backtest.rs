use crate::engine::portfolio::select_top_bottom_k;

pub fn run_backtest(
    scores_series: &Vec<Vec<f64>>,
    returns_series: &Vec<Vec<f64>>
) -> Vec<f64> {

    let mut pnl_series = Vec::new();

    // 初期ウェイト
    let mut prev_weights = vec![0.0; scores_series[0].len()];

    for t in 0..scores_series.len() {

        let scores = &scores_series[t];
        let returns = &returns_series[t];

        let k = 4;

        let (long_idx, short_idx) = select_top_bottom_k(scores, k);

        // =========================
        // weights
        // =========================
        let mut weights = vec![0.0; scores.len()];

        for &i in &long_idx {
            weights[i] = 1.0 / k as f64;
        }

        for &i in &short_idx {
            weights[i] = -1.0 / k as f64;
        }

        // =========================
        // turnover
        // =========================
        let turnover: f64 = weights.iter()
            .zip(prev_weights.iter())
            .map(|(w, pw)| (w - pw).abs())
            .sum();

        // =========================
        // pnl
        // =========================
        let mut pnl = 0.0;

        for i in 0..returns.len() {
            pnl += weights[i] * returns[i];
        }

        // =========================
        // cost
        // =========================
        let cost = turnover * 0.001;
        pnl -= cost;

        println!("pnl: {}", pnl);

        prev_weights = weights;
        pnl_series.push(pnl);
    }

    // 👇 これがないとエラーになる
    pnl_series
}
pub fn sharpe(pnl: &Vec<f64>) -> f64 {
    let mean: f64 = pnl.iter().sum::<f64>() / pnl.len() as f64;

    let var: f64 = pnl.iter()
        .map(|x| (x - mean).powi(2))
        .sum::<f64>() / pnl.len() as f64;

    mean / (var.sqrt() + 1e-8)
}