mod api;
mod engine;
mod model;
mod analysis;

use analysis::alpha_analysis::*;
use crate::api::yahoo::fetch_ohlc;
use crate::engine::backtest::{run_backtest, sharpe};
use crate::engine::features::compute_features;
use crate::engine::alpha::compute_score;
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
fn rank_normalize(scores: &Vec<f64>) -> Vec<f64> {
    let mut pairs: Vec<(usize, f64)> =
        scores.iter().cloned().enumerate().collect();

    pairs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

    let n = scores.len();
    let mut ranks = vec![0.0; n];

    for (rank, (i, _)) in pairs.iter().enumerate() {
        ranks[*i] = rank as f64 / n as f64;
    }

    ranks
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
    // ② 並列でデータ取得
    // =========================
    let concurrency = 50;
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
    .buffer_unordered(20)
    .collect()
    .await;
    let results: Vec<(String, Vec<OHLC>)> = stream::iter(codes.clone())
        .map(|code| async move {
            let data = fetch_ohlc(&code).await;
            (code, data)
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    // =========================
    // ③ returns作成
    // =========================
    let mut all_returns: Vec<Vec<f64>> = Vec::new();

    for (code, data) in results {
        println!("{}: data len = {}", code, data.len());

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
    // ④ 長さを揃える（最重要）
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
    // ⑤ T×N に変換
    // =========================
    let mut returns_series = vec![vec![0.0; N]; T];

    for i in 0..N {
        for t in 0..T {
            returns_series[t][i] = all_returns[i][t];
        }
    }

    // =========================
// 🔥 シャッフル（正しい位置）
// =========================
let shuffle_flag = false; // ← trueでテスト

if shuffle_flag {
    println!("⚠️ CROSS-SECTION SHUFFLE ON");

    let mut rng = thread_rng();

    for t in 0..returns_series.len() {
        returns_series[t].shuffle(&mut rng);
    }
}

    // =========================
    // ⑥ features & scores
    // =========================
    let mut scores_series: Vec<Vec<f64>> = Vec::new();
let mut alpha_map: HashMap<String, Vec<f64>> = HashMap::new();
for t in 1..T {
    let returns = &returns_series[t - 1];

    // ===== 市場リターン =====
    let market_ret = returns.iter().sum::<f64>() / returns.len() as f64;

    // ===== クロスセクション分散 =====
    let cs_var = returns.iter()
        .map(|r| (r - market_ret).powi(2))
        .sum::<f64>() / returns.len() as f64;

    // ===== features =====
    let features = compute_features(returns);

    // ===== scores =====
    let scores: Vec<f64> = features
        .iter()
        .map(|f| compute_score(f))
        .collect();

    scores_series.push(scores);
}

    // =========================
    // ⑦ backtest
    // =========================
    let k = 50;

    let pnl_series = run_backtest(&scores_series, &returns_series, k);

    let s = sharpe(&pnl_series);

    let mid = pnl_series.len() / 2;

    let sharpe_first = sharpe(&pnl_series[..mid].to_vec());
    let sharpe_second = sharpe(&pnl_series[mid..].to_vec());

    println!("=========================");
    println!("📈 Sharpe Ratio: {:.4}", s);
    println!("⏱ First Half Sharpe: {:.4}", sharpe_first);
    println!("⏱ Second Half Sharpe: {:.4}", sharpe_second);

}