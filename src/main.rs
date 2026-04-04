mod api;
mod engine;
mod model;
mod analysis;
mod portfolio;

use portfolio::weight::softmax_weights;

use analysis::rank::rank;
use analysis::alpha_analysis::correlation;

use crate::api::yahoo::fetch_ohlc;
use crate::engine::backtest::{run_backtest, sharpe, compute_pnl};
use crate::model::ohlc::OHLC;

use std::fs::File;
use std::io::{BufRead, BufReader};

use futures::stream::{self, StreamExt};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// =========================
// CSV読み込み
// =========================
fn load_codes(path: &str) -> Vec<String> {
    let file = File::open(path).unwrap();
    let reader = BufReader::new(file);

    reader.lines()
        .skip(1)
        .filter_map(|line| line.ok())
        .filter_map(|line| {
            let cols: Vec<&str> = line.split(',').collect();
            cols.get(1).map(|c| format!("{}.T", c))
        })
        .collect()
}

// =========================
// MAIN
// =========================
#[tokio::main]
async fn main() {
let shuffle_flag = true;
    // =========================
    // 銘柄ロード
    // =========================
    let codes = load_codes("data/jpx_codes.csv");
    let codes: Vec<String> = codes.into_iter().take(1000).collect();

    println!("読み込み銘柄数: {}", codes.len());

    // =========================
    // データ取得
    // =========================
    let concurrency = 20;
    let counter = Arc::new(AtomicUsize::new(0));

    let results: Vec<(String, Vec<OHLC>)> = stream::iter(codes.clone())
        .map(|code| {
            let counter = counter.clone();
            async move {
                let data = fetch_ohlc(&code).await;
                let c = counter.fetch_add(1, Ordering::Relaxed);
                if c % 50 == 0 {
                    println!("progress: {}", c);
                }
                (code, data)
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    // =========================
    // returns作成
    // =========================
    let mut all_returns: Vec<Vec<f64>> = Vec::new();

    for (_code, data) in results {
        if data.len() < 200 {
            continue;
        }

        let prices: Vec<f64> = data.iter().map(|x| x.close).collect();

        let returns: Vec<f64> =
            prices.windows(2).map(|w| w[1] / w[0] - 1.0).collect();

        all_returns.push(returns);
    }

    println!("実際に使う銘柄数: {}", all_returns.len());

    if all_returns.is_empty() {
        return;
    }

    // =========================
    // 長さ揃え
    // =========================
    let min_len = all_returns.iter().map(|v| v.len()).min().unwrap();

    let all_returns: Vec<Vec<f64>> =
        all_returns.iter().map(|v| v[..min_len].to_vec()).collect();

    let T = min_len;
    let N = all_returns.len();

    // =========================
    // T×N変換
    // =========================
    let mut returns_series = vec![vec![0.0; N]; T];

    for i in 0..N {
        for t in 0..T {
            returns_series[t][i] = all_returns[i][t];
        }
    }

    // =========================
    // 市場リターン
    // =========================
    let mut market_returns = vec![0.0; T];
    for t in 0..T {
        market_returns[t] =
            returns_series[t].iter().sum::<f64>() / N as f64;
    }

    // =========================
    // アルファ生成
    // =========================
    let lookback = 10;

    let mut scores_a = Vec::new();
    let mut scores_b = Vec::new();

    for t in lookback..T-1 {

        let returns = &returns_series[t];
        let mean = returns.iter().sum::<f64>() / N as f64;

        // ===== Alpha A =====
        let vol_scores: Vec<f64> =
            returns.iter().map(|r| (r - mean).abs()).collect();

        let mut rev_scores = vec![0.0; N];

        for i in 0..N {
            let r =
                returns_series[t][i]
              + returns_series[t-1][i]
              + returns_series[t-2][i];

            rev_scores[i] = -r;
        }

        let r_vol = rank(&vol_scores);
        let r_rev = rank(&rev_scores);

        let mut alpha_a = vec![0.0; N];

        for i in 0..N {
            if r_vol[i] > 0.8 {
                alpha_a[i] = r_rev[i];
            }
        }

        // ===== Alpha B =====
        let mut trend_scores = vec![0.0; N];

        for i in 0..N {

            let short =
                0.6 * returns_series[t][i] +
                0.3 * returns_series[t-1][i] +
                0.1 * returns_series[t-2][i];

            let mut long = 0.0;
            let mut w_sum = 0.0;

            for j in 0..10 {
                let w = (10 - j) as f64;
                long += w * returns_series[t-j][i];
                w_sum += w;
            }
            long /= w_sum;

            let mut vol = 0.0;
            for j in 0..5 {
                vol += returns_series[t-j][i].powi(2);
            }
            vol = vol.sqrt();

            trend_scores[i] = (short - long) / (vol + 1e-6);
        }

        let r_trend = rank(&trend_scores);

        scores_a.push(alpha_a);
        scores_b.push(r_trend);
    }

    // =========================
    // 単体バックテスト
    // =========================
    let top_k = 50;

    let pnl_a = run_backtest(&scores_a, &returns_series[lookback..].to_vec(), top_k);
    let pnl_b = run_backtest(&scores_b, &returns_series[lookback..].to_vec(), top_k);

    let sharpe_a = sharpe(&pnl_a);
    let sharpe_b = sharpe(&pnl_b);

    println!("Alpha A Sharpe: {:.4}", sharpe_a);
    println!("Alpha B Sharpe: {:.4}", sharpe_b);

    // =========================
    // 最終PnL（ここが本質）
    // =========================
// =========================
// シグナル統合（AのみでOK）
// =========================
let mut scores_final: Vec<Vec<f64>> = Vec::new();

for t in 0..scores_a.len() {
    scores_final.push(scores_a[t].clone());
}

// =========================
// バックテスト（ここだけ）
// =========================
let pnl_final = run_backtest(
    &scores_final,
    &returns_series[lookback..].to_vec(),
    top_k
);

    // =========================
    // 結果
    // =========================
    println!("=========================");
    println!("📈 Final Sharpe: {:.4}", sharpe(&pnl_final));
    println!("Correlation A vs B: {:.4}", correlation(&pnl_a, &pnl_b));
}