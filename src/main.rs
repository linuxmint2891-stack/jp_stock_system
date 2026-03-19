use reqwest::Client;
use futures::stream::{self, StreamExt};
use std::fs::File;
use std::io::Write;

mod api;
mod model;
mod indicator;
mod collector;

use api::yahoo::fetch_ohlc;
use indicator::ma::sma;
use indicator::rsi::rsi;
use crate::model::ohlc::OHLC;

use csv::ReaderBuilder;


// =========================
// CSV読み込み関数
// =========================
async fn load_codes() -> Vec<String> {
    let path = "/home/michiharu/my_project/jp_stock_system/data/jpx_codes.csv";
    let mut codes = Vec::new();

    // csv クレートで読み込む
    let mut rdr = ReaderBuilder::new()
        .delimiter(b',') // カンマ区切り
        .has_headers(true)
        .from_path(path)
        .expect("CSVファイルが開けません");

    for result in rdr.records() {
        let record = result.expect("CSVレコード読み込み失敗");
        if let Some(code) = record.get(1) {
            let code = code.trim();
            if !code.is_empty() {
                codes.push(code.to_string());
            }
        }
    }

    codes
}

// =========================
// main
// =========================
#[tokio::main]
async fn main() {
    // ✅ CSV読み込み済みコードを取得
    let codes = load_codes().await;
    println!("codes: {:?}", &codes[0..10.min(codes.len())]);
    println!("銘柄数: {}", codes.len());

    let client = Client::new();
    let start = "2022-01-01";

    // =========================
    // ① OHLC取得（非同期）
    // =========================
    let ohlc_results: Vec<(String, Vec<OHLC>)> = stream::iter(codes.clone())
        .then(|code: String| {
            let client = client.clone();
            async move {
                let symbol = format!("{}.T", code); // ← ここに入れる
                let data = fetch_ohlc(&symbol).await;
                (code, data)
            }
        })
        .collect()
        .await;

    println!("全銘柄のOHLC取得完了");

    // =========================
    // ② スコア計算
    // =========================
    let scored: Vec<(String, f64, f64)> = ohlc_results
        .iter()
        .map(|(code, rows): &(String, Vec<OHLC>)| {
            let closes: Vec<f64> = rows.iter().map(|r| r.close).collect();

            if closes.len() < 30 {
                return (code.clone(), 0.0, 0.0);
            }

            let ma_short = sma(&closes, 5).last().cloned().flatten().unwrap_or(0.0);
            let ma_long = sma(&closes, 25).last().cloned().flatten().unwrap_or(0.0);
            let rsi_val = rsi(&closes, 14).last().cloned().flatten().unwrap_or(50.0);

            let last = *closes.last().unwrap();

            let trend = (ma_short - ma_long) / ma_long;
            let momentum = (last - closes[closes.len() - 5]) / closes[closes.len() - 5];

            let rsi_score = if rsi_val < 30.0 {
                1.0
            } else if rsi_val > 70.0 {
                -1.0
            } else {
                0.0
            };

            let score = trend * 0.5 + momentum * 0.3 + rsi_score * 0.2;

            let future_return =
                (closes[closes.len() - 1] - closes[closes.len() - 5])
                    / closes[closes.len() - 5];

            (code.clone(), score, future_return)
        })
        .collect();

    println!("指標計算完了");

    // =========================
    // ③ 上位銘柄
    // =========================
    let mut top_stocks = scored.clone();
    top_stocks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    println!("上位銘柄:");
    for (code, score, _) in &top_stocks {
        let signal = judge(*score);
        println!("{} => {:.3} [{}]", code, score, signal);
    }

    // =========================
    // ④ 勝率計算
    // =========================
    let threshold = 0.02;

    let buy_selected: Vec<_> = scored
        .iter()
        .filter(|(_, score, _)| *score > threshold)
        .collect();

    let buy_wins = buy_selected
        .iter()
        .filter(|(_, _, ret)| *ret > 0.0)
        .count();

    let short_selected: Vec<_> = scored
        .iter()
        .filter(|(_, score, _)| *score < -threshold)
        .collect();

    let short_wins = short_selected
        .iter()
        .filter(|(_, _, ret)| *ret < 0.0)
        .count();

    if !buy_selected.is_empty() {
        println!(
            "BUY勝率: {:.2}%",
            buy_wins as f64 / buy_selected.len() as f64 * 100.0
        );
    }

    // =========================
    // ④ 閾値最適化
    // =========================
    println!("\n=== 閾値最適化 ===");

    let mut best_threshold = 0.0;
    let mut best_winrate = 0.0;

    for i in 1..10 {
        let threshold = i as f64 * 0.01;

        let selected: Vec<_> = scored
            .iter()
            .filter(|(_, score, _)| score.abs() > threshold)
            .collect();

        if selected.len() < 2 {
            continue;
        }

        let wins = selected
            .iter()
            .filter(|(_, score, ret)| {
                (*score > 0.0 && *ret > 0.0) ||
                (*score < 0.0 && *ret < 0.0)
            })
            .count();

        let winrate = wins as f64 / selected.len() as f64;

        println!(
            "threshold {:.2} → 勝率 {:.2}% ({}件)",
            threshold,
            winrate * 100.0,
            selected.len()
        );

        if winrate > best_winrate {
            best_winrate = winrate;
            best_threshold = threshold;
        }
    }

    println!(
        "\n🔥 最適閾値: {:.2} / 勝率 {:.2}%",
        best_threshold,
        best_winrate * 100.0
    );

    if !short_selected.is_empty() {
        println!(
            "SHORT勝率: {:.2}%",
            short_wins as f64 / short_selected.len() as f64 * 100.0
        );
    }

    // =========================
    // ⑤ 売買候補
    // =========================
    println!("BUY候補:");
    for (code, score, _) in buy_selected {
        println!("{} {:.3}", code, score);
    }

    println!("SHORT候補:");
    for (code, score, _) in short_selected {
        println!("{} {:.3}", code, score);
    }

    // =========================
    // ⑥ CSV出力
    // =========================
    let mut file = File::create("result.csv").unwrap();

    writeln!(file, "code,score,future_return").unwrap();

    for (code, score, ret) in &scored {
        writeln!(file, "{},{:.5},{:.5}", code, score, ret).unwrap();
    }

    println!("CSV出力完了");

    println!("全処理完了 🚀");
}

// =========================
// 判定関数
// =========================
fn judge(score: f64) -> &'static str {
    if score > 0.02 {
        "BUY"
    } else if score < -0.02 {
        "SELL"
    } else {
        "HOLD"
    }
}