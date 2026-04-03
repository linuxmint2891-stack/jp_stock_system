mod api;
mod engine;
mod model;
mod analysis;

use analysis::rank::rank;
use analysis::alpha_analysis::*;

use crate::api::yahoo::fetch_ohlc;
use crate::engine::backtest::{run_backtest, sharpe};
use crate::model::ohlc::OHLC;

use std::fs::File;
use std::io::{BufRead, BufReader};

use futures::stream::{self, StreamExt};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use rand::seq::SliceRandom;
use rand::thread_rng;

// =========================
// CSVから銘柄読み込み
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

    // =========================
    // ① 銘柄ロード
    // =========================
    let codes = load_codes("data/jpx_codes.csv");
    println!("読み込み銘柄数: {}", codes.len());

    // =========================
    // ② データ取得
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
    // ③ returns作成
    // =========================
    let mut all_returns: Vec<Vec<f64>> = Vec::new();

    for (_code, data) in results {
        if data.len() < 200 {
            continue;
        }

        let prices: Vec<f64> = data.iter().map(|x| x.close).collect();

        let returns: Vec<f64> = prices
            .windows(2)
            .map(|w| w[1] / w[0] - 1.0)
            .collect();

        all_returns.push(returns);
    }

    println!("実際に使う銘柄数: {}", all_returns.len());

    if all_returns.is_empty() {
        println!("❌ データなし");
        return;
    }

    // =========================
    // ④ 長さ揃え
    // =========================
    let min_len = all_returns.iter().map(|v| v.len()).min().unwrap();
    println!("共通長: {}", min_len);

    let all_returns: Vec<Vec<f64>> = all_returns
        .iter()
        .map(|v| v[..min_len].to_vec())
        .collect();

    let T = min_len;
    let N = all_returns.len();

    // =========================
    // ⑤ T×N変換
    // =========================
    let mut returns_series = vec![vec![0.0; N]; T];

    for i in 0..N {
        for t in 0..T {
            returns_series[t][i] = all_returns[i][t];
        }
    }

    // =========================
    // シャッフル（検証用）
    // =========================
    let shuffle_flag = false;

    if shuffle_flag {
        println!("⚠️ SHUFFLE ON");
        let mut rng = thread_rng();
        for t in 0..returns_series.len() {
            returns_series[t].shuffle(&mut rng);
        }
    }

// =========================
// ⑥ アルファ生成
// =========================
let lookback = 10;

let mut scores_a: Vec<Vec<f64>> = Vec::new();
let mut scores_b: Vec<Vec<f64>> = Vec::new();

for t in lookback..T-1 {

    // ===== Alpha A =====
    let returns = &returns_series[t];

    let mean = returns.iter().sum::<f64>() / N as f64;

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

    // ===== Alpha B（Trend）=====
    let mut trend_scores = vec![0.0; N];

    for i in 0..N {

        // short
        let short =
            0.6 * returns_series[t][i] +
            0.3 * returns_series[t-1][i] +
            0.1 * returns_series[t-2][i];

        // long
        let mut long = 0.0;
        let mut w_sum = 0.0;

        for j in 0..10 {
            let w = (10 - j) as f64;
            long += w * returns_series[t-j][i];
            w_sum += w;
        }
        long /= w_sum;

        // vol
        let mut vol: f64 = 0.0;
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
// ⑦ バックテスト
// =========================
let top_k: usize = 5;

let pnl_a = run_backtest(
    &scores_a,
    &returns_series[lookback..].to_vec(),
    top_k
);

let pnl_b = run_backtest(
    &scores_b,
    &returns_series[lookback..].to_vec(),
    top_k
);

// =========================
// 相関・分散
// =========================
let corr = correlation(&pnl_a, &pnl_b);

fn variance(v: &Vec<f64>) -> f64 {
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / v.len() as f64
}

let var_a = variance(&pnl_a);
let var_b = variance(&pnl_b);

// =========================
// 重み（スカラー）
// =========================
let mut w_a: f64 = (1.0 / var_a) * (1.0 - corr);
let mut w_b: f64 = (1.0 / var_b) * (1.0 - corr);

let sum = w_a + w_b + 1e-6;
w_a /= sum;
w_b /= sum;

println!("weight A: {:.3}, weight B: {:.3}", w_a, w_b);

// =========================
// 最終PnL
// =========================
let mut pnl_final = vec![0.0; pnl_a.len()];

for t in 0..pnl_a.len() {
    pnl_final[t] = w_a * pnl_a[t] + w_b * pnl_b[t];
}

// =========================
// 結果
// =========================
println!("=========================");
println!("📈 Alpha A Sharpe: {:.4}", sharpe(&pnl_a));
println!("📈 Alpha B Sharpe: {:.4}", sharpe(&pnl_b));
println!("📈 Final Sharpe: {:.4}", sharpe(&pnl_final));
println!("Correlation A vs B: {:.4}", corr);
}