use polars::prelude::*;

fn main() -> PolarsResult<()> {
    let df = LazyFrame::scan_parquet("data/processed_market_data.parquet", Default::default())?
        .select([col("Code")])
        .unique(None, UniqueKeepStrategy::First)
        .collect()?;
    
    let codes = df.column("Code")?.str()?;
    for code in codes.into_iter().flatten().take(10) {
        println!("{}", code);
    }
    println!("Total distinct codes: {}", codes.len());
    Ok(())
}