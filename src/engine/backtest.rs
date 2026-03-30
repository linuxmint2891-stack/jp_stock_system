use crate::engine::portfolio::select_top_bottom_k;

pub fn run_backtest(
    scores_series: &Vec<Vec<f64>>,
    returns_series: &Vec<Vec<f64>>,
    k: usize
) -> Vec<f64> {

    let mut pnl_series = Vec::new();

    if scores_series.is_empty() {
        println!("❌ scores_series empty");
        return pnl_series;
    }

    for t in 0..scores_series.len() {

        if t + 1 >= returns_series.len() {
            break;
        }

        let scores = &scores_series[t];
        let returns_next = &returns_series[t + 1];

        let (long_idx, short_idx) = select_top_bottom_k(scores, k);

        let mut pnl = 0.0;
// スコアをそのまま重みに使う
let mut total_weight = 0.0;

for &i in &long_idx {
    let w = scores[i].abs();
    pnl += returns_next[i] * w;
    total_weight += w;
}

for &i in &short_idx {
    let w = scores[i].abs();
    pnl -= returns_next[i] * w;
    total_weight += w;
}

pnl /= total_weight.max(1e-8);

        pnl_series.push(pnl);
    }

    pnl_series
}

pub fn sharpe(pnl: &Vec<f64>) -> f64 {
    let mean = pnl.iter().sum::<f64>() / pnl.len() as f64;

    let var = pnl.iter()
        .map(|x| (x - mean).powi(2))
        .sum::<f64>() / pnl.len() as f64;

    mean / (var.sqrt() + 1e-8)
}