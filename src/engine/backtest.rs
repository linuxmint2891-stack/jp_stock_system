use crate::engine::portfolio::select_top_bottom_k;

// =========================
// Backtest（シンプル版）
// =========================
pub fn run_backtest(
    scores_series: &Vec<Vec<f64>>,
    returns_series: &Vec<Vec<f64>>,
    k: usize,
) -> Vec<f64> {

    let mut pnl_series = Vec::new();

    let t_max = scores_series.len();

    for t in 0..t_max - 1 {

        let scores = &scores_series[t];
        let returns_next = &returns_series[t + 1];

        let (long_idx, short_idx) = select_top_bottom_k(scores, k);

        let mut pnl = 0.0;

        for &i in &long_idx {
            pnl += returns_next[i];
        }

        for &i in &short_idx {
            pnl -= returns_next[i];
        }

        pnl /= (k * 2) as f64;

        pnl_series.push(pnl);
    }

    pnl_series
}

// =========================
// Sharpe
// =========================
pub fn sharpe(pnl: &Vec<f64>) -> f64 {
    let mean = pnl.iter().sum::<f64>() / pnl.len() as f64;

    let var = pnl.iter()
        .map(|x| (x - mean).powi(2))
        .sum::<f64>() / pnl.len() as f64;

    mean / (var.sqrt() + 1e-8)
}