use polars::prelude::*;
use std::env;

fn main() -> PolarsResult<()> {
    let args: Vec<String> = env::args().collect();
    let path = if args.len() > 1 {
        &args[1]
    } else {
        "data/processed_market_data.parquet"
    };

    println!("📖 Reading Parquet: {}", path);

    let df = LazyFrame::scan_parquet(path, Default::default())?
        .collect()?;
    
    // スキーマも表示
    println!("\n📊 Schema: {:?}", df.schema());
    println!("📈 Total rows: {}", df.height());

    // 39230 のニュースが入っているか確認
    let raks = df.clone().lazy().filter(col("Code").eq(lit("39230"))).limit(1).collect()?;
    println!("🔍 Sample for 39230:\n{}", raks);

    // 最初の 10 行を表示
    println!("{}", df.head(Some(10)));
    
    Ok(())
}
