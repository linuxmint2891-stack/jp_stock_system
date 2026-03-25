mod engine;

use crate::engine::backtest::{run_backtest, sharpe};
use crate::engine::features::compute_features;
use crate::engine::alpha::compute_score;

fn main() {
    // =========================
    // ① 時系列コンテナ
    // =========================
    let mut scores_series: Vec<Vec<f64>> = Vec::new();
    let mut returns_series: Vec<Vec<f64>> = Vec::new();

    let T = 100;

    // =========================
    // ② 時系列ループ（最重要）
    // =========================
    for t in 0..T {

    let returns = compute_returns(t);

    let features = compute_features(&returns);

    let scores: Vec<f64> = features
        .iter()
        .map(|f| compute_score(f))
        .collect();

        println!("returns len: {}", returns.len());
        scores_series.push(scores);
        returns_series.push(returns);
    }
    fn compute_scores(features: &Vec<f64>) -> Vec<f64> {
        features
            .iter()
            .map(|x| x.tanh()) // ← signum削除
            .collect()
    }
    // =========================
    // ③ Backtest
    // =========================
    let pnl_series = run_backtest(&scores_series, &returns_series);

    // =========================
    // ④ Sharpe
    // =========================
    let s = sharpe(&pnl_series);

    let mid = pnl_series.len() / 2;

    let sharpe_first = sharpe(&pnl_series[..mid].to_vec());
    let sharpe_second = sharpe(&pnl_series[mid..].to_vec());

    println!("📈 Sharpe Ratio: {:.4}", s);
    println!("⏱ First Half Sharpe: {:.4}", sharpe_first);
    println!("⏱ Second Half Sharpe: {:.4}", sharpe_second);
}

//
// =========================
// ▼ ダミーデータ（必須）
// =========================
//

// --- returns生成（時系列変化あり）
fn compute_returns(t: usize) -> Vec<f64> {
    vec![
        0.01 * ((t as f64 + 1.0) * 0.5).sin(),
        0.01 * ((t as f64 + 2.0) * 0.3).cos(),
        0.01 * ((t as f64 + 3.0) * 0.7).sin(),
        0.01 * ((t as f64 + 4.0) * 0.2).cos(),
        0.01 * ((t as f64 + 5.0) * 0.9).sin(),
    ]
}

// --- スコア（returnsに相関を持たせる）
fn compute_scores(features: &Vec<f64>) -> Vec<f64> {
    features.iter().map(|x| x.tanh()).collect()
}
