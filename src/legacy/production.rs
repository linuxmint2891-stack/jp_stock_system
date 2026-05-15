use polars::prelude::*;
use anyhow::Result;

fn main() -> Result<()> {
    println!("🚀 Starting Market Analysis (J-Quants + Yahoo)...");

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
    let jquants_lf = LazyFrame::scan_parquet(market_data_path, Default::default())?
        .select([
            col("Date"),
            col("Code"), 
            col("Code").str().slice(lit(0), lit(4)).alias("ShortCode"),
            col("AdjC"),
            col("Va"),    // 流動性フィルタ用
            col("AdjVo"), // 出来高用
        ]);

    // Yahooデータがあれば読み込む
    let mut market_lfs = vec![jquants_lf];
    if std::path::Path::new(yahoo_data_path).exists() {
        println!("📈 Loading Yahoo latest data...");
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
            ]);
        market_lfs.push(yahoo_lf);
    }

    let combined_lf = concat(market_lfs, UnionArgs::default())?;

    // 3. 計算（前日比、移動平均）と結合
    let base_lf = combined_lf
        .sort(["ShortCode", "Date"], SortMultipleOptions::default())
        .with_columns([
            // 前日比 (%)
            ((col("AdjC") / col("AdjC").shift(lit(1)) - lit(1.0)) * lit(100.0))
                .over([col("ShortCode")])
                .alias("Change(%)"),
            // 5日移動平均
            col("AdjC")
                .rolling_mean(RollingOptionsFixedWindow {
                    window_size: 5,
                    min_periods: 1,
                    ..Default::default()
                })
                .over([col("ShortCode")])
                .alias("MA5"),
            // 20日移動平均
            col("AdjC")
                .rolling_mean(RollingOptionsFixedWindow {
                    window_size: 20,
                    min_periods: 1,
                    ..Default::default()
                })
                .over([col("ShortCode")])
                .alias("MA20"),
        ])
        .left_join(master_lf, col("ShortCode"), col("JoinCode"));

    // 4. ランキング抽出 & デバッグ情報
    println!("Extracting rankings...");
    
    // 最新の日付を特定（フィルタ用）
    let latest_date_df = base_lf.clone().select([col("Date").max()]).collect()?;
    let latest_date_av = latest_date_df.column("Date")?.get(0)?;
    let latest_date_str = latest_date_av.to_string().replace("\"", "");
    println!("Latest date in data: {}", latest_date_str);

    // デバッグ1: データが2行以上ある銘柄数を確認
    let row_counts = base_lf.clone()
        .group_by([col("Code")])
        .agg([col("Date").count().alias("row_count")])
        .filter(col("row_count").gt(lit(1)))
        .collect()?;
    println!("📊 計算可能な銘柄数: {}", row_counts.height());

    // フィルタリングを適用した基幹データ
    let ranking_lf = base_lf
        .filter(col("Date").eq(lit(latest_date_str.clone())))
        .filter(col("Change(%)").gt(lit(-30.0)).and(col("Change(%)").lt(lit(30.0))))
        .filter(
            col("Name").str().contains(lit("種類株"), false).not()
            .and(col("Name").str().contains(lit("優先株"), false).not())
            .and(col("Name").str().contains(lit("受益証券"), false).not())
        )
        .drop_nulls(Some(vec!["Change(%)".into()]));

    // --- 値上がり率ランキング ---
    let top_gainers = ranking_lf.clone()
        .sort(["Change(%)"], SortMultipleOptions::default().with_order_descending(true))
        .limit(10)
        .collect()?;

    // --- 値下がり率ランキング ---
    let top_losers = ranking_lf.clone()
        .sort(["Change(%)"], SortMultipleOptions::default().with_order_descending(false))
        .limit(10)
        .collect()?;

    // --- 【Scout】上昇トレンドの低位株（Buy Signals） ---
    let scout_results = ranking_lf.clone()
        .filter(
            col("AdjC").lt(lit(1000.0))           // 低位株
            .and(col("Va").gt(lit(50_000_000.0))) // 流動性あり
            .and(col("AdjC").gt(col("MA5")))      // 短期上昇（5日線の上）
            .and(col("Change(%)").gt(lit(0.0)))    // 続伸中
        )
        .sort(["Change(%)"], SortMultipleOptions::default().with_order_descending(true))
        .limit(10)
        .collect()?;
    
    // 5. 表示
    println!("\n🔥 【値上がり率ランキング（対象日: {}）】", latest_date_str);
    if top_gainers.height() > 0 {
        println!("{}", top_gainers.select(["Code", "Name", "Sector", "AdjC", "Change(%)"])?);
    } else {
        println!("(No data available for ranking)");
    }

    println!("\n❄️ 【値下がり率ランキング（対象日: {}）】", latest_date_str);
    if top_losers.height() > 0 {
        println!("{}", top_losers.select(["Code", "Name", "Sector", "AdjC", "Change(%)"])?);
    } else {
        println!("(No data available for ranking)");
    }

    println!("\n✨ 【Scout: 上昇トレンドの注目銘柄（対象日: {}）】", latest_date_str);
    if scout_results.height() > 0 {
        println!("{}", scout_results.select(["Code", "Name", "Sector", "AdjC", "Change(%)", "MA5"])?);
    } else {
        println!("(No potential buy signals found)");
    }

    Ok(())
}