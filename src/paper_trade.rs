use polars::prelude::*;
use std::fs::File;
use std::path::Path;
use chrono::Local;

const PORTFOLIO_PATH: &str = "data/paper_portfolio.parquet";

/// 仮想ポートフォリオに新規エントリー（買い建て）を記録する
pub fn record_virtual_buy(
    code: &str,
    name: &str,
    entry_price: f64,
    qty: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    
    // 新規レコードの作成
    let df_new = df!(
        "code" => &[code],
        "name" => &[name],
        "entry_date" => &[today.as_str()],
        "entry_price" => &[entry_price],
        "current_price" => &[entry_price], // 初期値は購入時と同じ
        "qty" => &[qty],
        "profit_loss" => &[0.0],           // 初期評価損益は 0.0
        "status" => &["HOLD"]
    )?;

    let mut df_to_save = if Path::new(PORTFOLIO_PATH).exists() {
        // 既存の台帳があればロードして結合
        let file = File::open(PORTFOLIO_PATH)?;
        let df_existing = ParquetReader::new(file).finish()?;
        
        // すでに同一銘柄をHOLDしている場合は、単純に追加（Vstack）
        df_existing.vstack(&df_new)?
    } else {
        df_new
    };

    let file = File::create(PORTFOLIO_PATH)?;
    ParquetWriter::new(file)
        .with_compression(ParquetCompression::Snappy)
        .finish(&mut df_to_save)?;

    println!("📝 ペーパートレード台帳に記録しました: {} (購入単価: {}円)", name, entry_price);
    Ok(())
}

/// 最新の市場データ（market_data.parquet）を基に、保有中銘柄の評価損益を更新する
pub fn update_portfolio_prices(market_data_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !Path::new(PORTFOLIO_PATH).exists() {
        return Ok(()); // 台帳がなければ何もしない
    }

    println!("📈 保有中の仮想ポートフォリオの評価換算（マーク・トゥ・マーケット）を実行中...");

    // 1. 既存のポートフォリオ台帳をロード
    let df_portfolio = ParquetReader::new(File::open(PORTFOLIO_PATH)?).finish()?;
    
    // 2. 最新の市場価格データをロード
    let df_market = ParquetReader::new(File::open(market_data_path)?).finish()?;

    // 最新の現在値テーブルを作成
    let latest_date = {
        let date_col = df_market.column("Date")?.str()?;
        date_col.into_no_null_iter().max().unwrap_or("").to_string()
    };
    
    let df_latest_prices = df_market.lazy()
        .filter(col("Date").eq(lit(latest_date.as_str())))
        .select([col("Code").alias("code"), col("AdjC").alias("price")])
        .collect()?;

    // 3. 結合（Join）して最新株価をマッピング
    let df_merged = df_portfolio
        .drop("current_price")? // 古い価格列を一旦削除
        .join(
            &df_latest_prices,
            ["code"],
            ["code"],
            JoinArgs::new(JoinType::Left),
        )?;

    // 4. 新しい現在値（price）を用いて評価損益（profit_loss）を再計算
    let mut df_updated = df_merged
        .lazy()
        .with_column(col("price").alias("current_price"))
        .with_column(
            ((col("current_price") - col("entry_price")) * col("qty").cast(DataType::Float64))
                .alias("profit_loss")
        )
        .drop(["price"]) // 一時的な列を削除
        .collect()?;

    // 5. 更新された台帳を保存
    let file = File::create(PORTFOLIO_PATH)?;
    ParquetWriter::new(file)
        .with_compression(ParquetCompression::Snappy)
        .finish(&mut df_updated)?;

    println!("✅ ポートフォリオの評価損益を最新に更新しました。");
    Ok(())
}

/// ポートフォリオの内容を読み取ってリストで返す（TUI用）
pub fn load_portfolio() -> Result<DataFrame, Box<dyn std::error::Error>> {
    if !Path::new(PORTFOLIO_PATH).exists() {
        return Ok(DataFrame::empty());
    }
    let file = File::open(PORTFOLIO_PATH)?;
    let df = ParquetReader::new(file).finish()?;
    Ok(df)
}
