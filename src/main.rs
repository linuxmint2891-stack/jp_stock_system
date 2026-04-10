mod alpha;

fn alpha_a(f: &[f64]) -> f64 { f.iter().sum::<f64>() / f.len() as f64 }
fn alpha_b(f: &[f64]) -> f64 { if f.is_empty() { 0.0 } else { f[0] } }

// =========================
// Utility
// =========================
fn mean(x: &[f64]) -> f64 {
    x.iter().sum::<f64>() / x.len() as f64
}

fn std_dev(x: &[f64]) -> f64 {
    let m = mean(x);
    let var = x.iter().map(|v| (v - m).powi(2)).sum::<f64>() / x.len() as f64;
    var.sqrt()
}

fn softmax(x: &[f64]) -> Vec<f64> {
    let max = x.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let exp: Vec<f64> = x.iter().map(|v| (v - max).exp()).collect();
    let sum: f64 = exp.iter().sum();
    exp.iter().map(|v| v / sum).collect()
}

fn zscore(x: &[f64]) -> Vec<f64> {
    let m = mean(x);
    let s = std_dev(x);
    x.iter().map(|v| (v - m) / (s + 1e-8)).collect()
}

fn correlation(x: &[f64], y: &[f64]) -> f64 {
    let mx = mean(x);
    let my = mean(y);

    let cov = x.iter()
        .zip(y.iter())
        .map(|(a, b)| (a - mx) * (b - my))
        .sum::<f64>() / x.len() as f64;

    cov / (std_dev(x) * std_dev(y) + 1e-8)
}

fn sharpe(pnl: &[f64]) -> f64 {
    let m = mean(pnl);
    let s = std_dev(pnl);
    m / (s + 1e-8)
}

// =========================
// Main
// =========================
fn main() {
    println!("--- Time Series Backtest (IC + Sign Fix + Ensemble) ---");

    // ダミーデータ（time × stock × feature）
    let features_series = vec![
        vec![
            vec![0.1, 0.2, 0.3],
            vec![0.2, 0.1, 0.4],
            vec![0.3, 0.3, 0.3],
        ],
        vec![
            vec![0.2, 0.1, 0.2],
            vec![0.1, 0.3, 0.2],
            vec![0.4, 0.2, 0.1],
        ],
        vec![
            vec![0.3, 0.2, 0.1],
            vec![0.2, 0.2, 0.2],
            vec![0.1, 0.4, 0.3],
        ],
    ];

    let future_returns_series = vec![
        vec![0.01, -0.02, 0.03],
        vec![0.02, 0.01, -0.01],
        vec![-0.01, 0.03, 0.02],
    ];

    let mut returns_a = Vec::new();
    let mut returns_b = Vec::new();
    let mut returns_ens = Vec::new();

    for t in 0..features_series.len() {
        let features = &features_series[t];
        let future_returns = &future_returns_series[t];

        // =========================
        // スコア
        // =========================
        let scores_a: Vec<f64> = features.iter().map(|f| alpha_a(f)).collect();
        let scores_b: Vec<f64> = features.iter().map(|f| alpha_b(f)).collect();

        // =========================
        // IC
        // =========================
        let ic_a = correlation(&scores_a, future_returns);
        let ic_b = correlation(&scores_b, future_returns);

        let dir_a = if ic_a > 0.0 { 1.0 } else { -1.0 };
        let dir_b = if ic_b > 0.0 { 1.0 } else { -1.0 };

        // =========================
        // 正規化
        // =========================
        let z_a: Vec<f64> = zscore(&scores_a).iter().map(|x| x * dir_a).collect();
        let z_b: Vec<f64> = zscore(&scores_b).iter().map(|x| x * dir_b).collect();

        // =========================
        // 単体PnL
        // =========================
        let pnl_a: f64 = softmax(&z_a)
            .iter()
            .zip(future_returns.iter())
            .map(|(w, r)| w * r)
            .sum();

        let pnl_b: f64 = softmax(&z_b)
            .iter()
            .zip(future_returns.iter())
            .map(|(w, r)| w * r)
            .sum();

        // =========================
        // アンサンブル
        // =========================
        let w_a = ic_a.abs() / (ic_a.abs() + ic_b.abs() + 1e-8);
        let w_b = ic_b.abs() / (ic_a.abs() + ic_b.abs() + 1e-8);

        let scores_ens: Vec<f64> = z_a.iter()
            .zip(z_b.iter())
            .map(|(a, b)| w_a * a + w_b * b)
            .collect();

        let pnl_ens: f64 = softmax(&scores_ens)
            .iter()
            .zip(future_returns.iter())
            .map(|(w, r)| w * r)
            .sum();

        returns_a.push(pnl_a);
        returns_b.push(pnl_b);
        returns_ens.push(pnl_ens);

        println!("\n--- t={} ---", t);
        println!("IC A: {:.4}, IC B: {:.4}", ic_a, ic_b);
        println!("PnL A: {:.5}, PnL B: {:.5}, PnL Ens: {:.5}", pnl_a, pnl_b, pnl_ens);
    }

    println!("\n=== FINAL RESULT ===");
    println!("Sharpe A  : {:.4}", sharpe(&returns_a));
    println!("Sharpe B  : {:.4}", sharpe(&returns_b));
    println!("Sharpe Ens: {:.4}", sharpe(&returns_ens));
}