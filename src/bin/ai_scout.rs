use polars::prelude::*;
use anyhow::Result;
use jp_stock_system::api::approver::TradeApprover;
use std::fs::OpenOptions;
use std::io::Write;
use chrono::Local;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let discord_webhook_url = std::env::var("DISCORD_WEBHOOK_URL").ok();
    if let Some(ref url) = discord_webhook_url {
        println!("🔍 Debug: DISCORD_WEBHOOK_URL loaded (length: {})", url.len());
    } else {
        println!("⚠️ Debug: DISCORD_WEBHOOK_URL not found in environment");
    }

    // ログファイルの準備
    let log_path = "logs/ai_scout_results.txt";
    std::fs::create_dir_all("logs")?;
    let mut log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;

    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    writeln!(log_file, "\n==================================================")?;
    writeln!(log_file, "🚀 AI Scout Execution: {}", timestamp)?;
    writeln!(log_file, "==================================================")?;

    println!("🚀 Starting Market Analysis with AI Scout (Ollama)...");

    let market_data_path = "data/processed_market_data.parquet";
    let master_data_path = "data/jpx_codes.csv";
    let yahoo_data_path = "data/yahoo_latest.csv";

    // 1. マスターデータの読み込み
    let master_df = LazyCsvReader::new(master_data_path)
        .with_has_header(true)
        .finish()?
        .collect()?;

    let all_cols: Vec<String> = master_df.get_column_names().iter().map(|s| s.to_string()).collect();
    
    let code_col = all_cols.iter()
        .find(|s| s.as_str() == "コード" || s.as_str() == "Code")
        .cloned()
        .unwrap_or_else(|| all_cols[1].clone());

    let name_col = all_cols.iter()
        .find(|s| s.as_str() == "銘柄名" || s.as_str() == "Name")
        .cloned()
        .unwrap_or_else(|| all_cols[2].clone());

    let master_lf = master_df.lazy().select([
        col(&code_col).cast(DataType::String).str().slice(lit(0), lit(4)).alias("JoinCode"),
        col(&name_col).alias("Name"),
        col("33業種区分").alias("Sector"),
    ]);

    // 2. 市場データの読み込みと統合
    let schema = LazyFrame::scan_parquet(market_data_path, Default::default())?.schema()?;
    let has_news_col = schema.contains("news_text");

    let mut select_cols = vec![
        col("Date"),
        col("Code"), 
        col("Code").str().slice(lit(0), lit(4)).alias("ShortCode"),
        col("AdjC"),
        col("Va"),
        col("AdjVo"),
    ];
    if has_news_col {
        select_cols.push(col("news_text"));
    }

    let mut jquants_lf = LazyFrame::scan_parquet(market_data_path, Default::default())?
        .select(select_cols);

    if !has_news_col {
        jquants_lf = jquants_lf.with_column(lit("").alias("news_text"));
    }

    let mut market_lfs = vec![jquants_lf];
    if std::path::Path::new(yahoo_data_path).exists() {
        let yahoo_lf = LazyCsvReader::new(yahoo_data_path)
            .finish()?
            .lazy()
            .select([
                col("Date"),
                col("Code").cast(DataType::String).alias("Code"),
                col("Code").cast(DataType::String).str().slice(lit(0), lit(4)).alias("ShortCode"),
                col("AdjC"),
                col("Va").fill_null(lit(0.0)),
                col("AdjVo").fill_null(lit(0.0)),
                lit("").alias("news_text"), // Yahoo CSV にはニュースがない前提
            ]);
        market_lfs.push(yahoo_lf);
    }

    let combined_lf = concat(market_lfs, UnionArgs::default())?;

    // 3. 計算と結合
    let base_lf = combined_lf
        .sort(["ShortCode", "Date"], SortMultipleOptions::default())
        .with_columns([
            ((col("AdjC") / col("AdjC").shift(lit(1)) - lit(1.0)) * lit(100.0))
                .over([col("ShortCode")])
                .alias("Change(%)"),
            col("AdjC")
                .rolling_mean(RollingOptionsFixedWindow {
                    window_size: 5,
                    min_periods: 1,
                    ..Default::default()
                })
                .over([col("ShortCode")])
                .alias("MA5"),
        ])
        .left_join(master_lf, col("ShortCode"), col("JoinCode"));

    // 4. Scout抽出
    let latest_date_df = base_lf.clone().select([col("Date").max()]).collect()?;
    let latest_date_av = latest_date_df.column("Date")?.get(0)?;
    let latest_date_str = latest_date_av.to_string().replace("\"", "");

    let scout_candidates = base_lf
        .filter(col("Date").eq(lit(latest_date_str.clone())))
        .filter(
            col("AdjC").lt(lit(2000.0))           // 低・中位株
            .and(col("Va").gt(lit(100_000_000.0))) // 流動性あり
            .and(col("AdjC").gt(col("MA5")))      // 5日線の上
            .and(col("Change(%)").gt(lit(1.0)))    // 1%以上の上昇
        )
        .sort(["Change(%)"], SortMultipleOptions::default().with_order_descending(true))
        .limit(10) // ニュース取得対象を少し多めに取る
        .collect()?;
    
    if scout_candidates.height() == 0 {
        println!("(No potential buy signals found)");
        return Ok(());
    }

    // --- 追加: リアルタイムニュースの取得とマージ ---
    let candidate_codes: Vec<String> = scout_candidates.column("Code")?
        .str()?
        .into_no_null_iter()
        .map(|s| s.to_string())
        .collect();

    println!("\n📡 抽出された銘柄の最新ニュースをリアルタイム取得します...");
    let real_news = jp_stock_system::news_crawler::fetch_real_news_for_codes(&candidate_codes).await;
    
    // 取得したニュースをParquetにマージ（永続化）
    if !real_news.is_empty() {
        jp_stock_system::news_merger::merge_news_to_parquet(market_data_path, real_news.clone())?;
        println!("✅ 最新ニュースを {} にマージしました。", market_data_path);
    }

    // マージ後のデータで再度フィルタリング（news_textを反映させるため）
    // ※今回は簡略化のため、取得したニュースを直接ループ内で参照するようにします
    let mut news_map = std::collections::HashMap::new();
    for n in real_news {
        news_map.insert(n.code, n.news_text);
    }

    println!("\n✨ 【Scout: 上昇トレンドの注目銘柄 (対象日: {})】", latest_date_str);

    // 5. AIによる分析の追加
    let codes = scout_candidates.column("Code")?.str()?;
    let names = scout_candidates.column("Name")?.str()?;
    let prices = scout_candidates.column("AdjC")?.f64()?;
    let changes = scout_candidates.column("Change(%)")?.f64()?;
    // let news_texts = scout_candidates.column("news_text")?.str()?; // Parquetから読み込んだ古いデータ

    for i in 0..scout_candidates.height() {
        let code = codes.get(i).unwrap_or("");
        let name = names.get(i).unwrap_or("");
        let price = prices.get(i).unwrap_or(0.0);
        let change = changes.get(i).unwrap_or(0.0);
        
        // 最新ニュースがあればそれを使う、なければ空文字
        let news_text = news_map.get(code).map(|s| s.as_str()).unwrap_or("");

        let header = format!("\n--- [銘柄分析] {} {} (価格: {}円, 前日比: {:.2}%) ---", code, name, price, change);
        println!("{}", header);
        writeln!(log_file, "{}", header)?;
        
        match TradeApprover::approve_trade(code, name, price, change, news_text).await {
            Ok(result) => {
                let res_text = format!("🤖 AI判定: {} (スコア: {:.2})\n📝 理由: {}\n⚠️ リスク: {}", 
                    result.decision, result.sentiment_score, result.reasons.join(" / "), result.risk_factor);
                println!("{}", res_text);
                writeln!(log_file, "{}", res_text)?;

                // Discord通知 (GO判定の場合のみ)
                if result.decision == "GO" {
                    if let Some(ref url) = discord_webhook_url {
                        if let Err(e) = jp_stock_system::notifier::send_discord_notification(
                            url,
                            code,
                            name,
                            price,
                            result.sentiment_score,
                            &result.reasons.join(" / "),
                            &result.risk_factor,
                        ).await {
                            eprintln!("⚠️ Discord通知失敗: {}", e);
                        }
                    }

                    // ペーパートレード台帳への記録 (スコア 0.70 以上)
                    if result.sentiment_score >= 0.70 {
                        if let Err(e) = jp_stock_system::paper_trade::record_virtual_buy(
                            code,
                            name,
                            price,
                            100 // 100株
                        ) {
                            eprintln!("❌ ペーパートレード記録失敗: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                let err_msg = format!("⚠️ AI分析失敗: {}", e);
                eprintln!("{}", err_msg);
                writeln!(log_file, "{}", err_msg)?;
            }
        }
    }

    Ok(())
}
