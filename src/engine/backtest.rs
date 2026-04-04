pub fn run_backtest(
    scores_series: &Vec<Vec<f64>>,
    returns_series: &Vec<Vec<f64>>,
    k: usize
) -> Vec<f64> {

    let mut pnl_series = Vec::new();

    for t in 0..scores_series.len() {

        if t + 1 >= returns_series.len() {
            break;
        }

        let scores = &scores_series[t];
        let returns_next = &returns_series[t + 1];

        let (long_idx, short_idx) = select_top_bottom_k(scores, k);

        let mut pnl = 0.0;

        // ロング
        for &i in &long_idx {
            pnl += returns_next[i];
        }

        // ショート
        for &i in &short_idx {
            pnl -= returns_next[i];
        }

        pnl /= (2.0 * k as f64);

        pnl_series.push(pnl);
    }

    pnl_series
}
pub fn compute_pnl(
    scores: &Vec<f64>,
    returns: &Vec<f64>,
    top_k: usize,
) -> f64 {

    let mut idx: Vec<usize> = (0..scores.len()).collect();

    idx.sort_by(|&i, &j| scores[j].partial_cmp(&scores[i]).unwrap());

    let mut pnl = 0.0;

    // ロング
    for &i in idx.iter().take(top_k) {
        pnl += returns[i];
    }

    // ショート
    for &i in idx.iter().rev().take(top_k) {
        pnl -= returns[i];
    }

    pnl / (2.0 * top_k as f64)
}
pub fn sharpe(pnl: &Vec<f64>) -> f64 {
    let mean = pnl.iter().sum::<f64>() / pnl.len() as f64;

    let var = pnl.iter()
        .map(|x| (x - mean).powi(2))
        .sum::<f64>() / pnl.len() as f64;

    mean / (var.sqrt() + 1e-8)
}
fn select_top_bottom_k(
    scores: &Vec<f64>,
    k: usize
) -> (Vec<usize>, Vec<usize>) {

    let mut idx: Vec<usize> = (0..scores.len()).collect();

    idx.sort_by(|&i, &j| scores[j].partial_cmp(&scores[i]).unwrap());

    let long = idx.iter().take(k).cloned().collect();
    let short = idx.iter().rev().take(k).cloned().collect();

    (long, short)
}