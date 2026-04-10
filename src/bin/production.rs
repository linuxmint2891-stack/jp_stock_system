use polars::prelude::*;

fn main() -> PolarsResult<()> {
    println!("Loading market data and master data...");
    
    let df_market = LazyFrame::scan_parquet("data/processed_market_data.parquet", ScanArgsParquet::default())?;
    let df_master = LazyCsvReader::new("data/jpx_codes.csv").with_has_header(true).finish()?;

    let df_collected = df_market.collect()?;
    let id_vars: Vec<&str> = vec!["Date"];
    let value_vars: Vec<&str> = Vec::new();

    let df_prices = df_collected.melt(id_vars, value_vars)?
        .lazy()
        .rename(["variable", "value"], ["FullCode", "Price"])
        // 先頭4文字をスライスして取得
        .with_column(
            col("FullCode").str().slice(lit(0), lit(4)).alias("Code")
        );

    // 4. 会社名を紐付け
    let final_df = df_prices
        .join(
            df_master,
            [col("Code")],        // 加工した4桁
            [col("コード")],       // CSV側の4桁
            JoinArgs::new(JoinType::Left),
        )
        .select([
            col("Code"),
            col("銘柄名"),
            col("Price"),
            col("17業種区分"),
        ])
        .collect()?;

    println!("--- 銘柄名付き価格リスト (Top 10) ---");
    println!("{}", final_df.head(Some(10)));

    Ok(())
}