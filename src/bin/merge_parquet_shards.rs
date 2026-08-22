use jp_stock_system::alpha::{alpha_a, alpha_b};
use polars::prelude::*;
use std::fs;

const RANGES: [&str; 3] = ["1000-3000", "3001-6000", "6001-9999"];

fn main() -> anyhow::Result<()> {
    let mut shards = Vec::new();
    for range in RANGES {
        let path = format!("data/processed_market_data_{range}.parquet");
        if !std::path::Path::new(&path).exists() {
            anyhow::bail!("統合対象の分割Parquetがありません: {path}");
        }
        shards.push(LazyFrame::scan_parquet(path, Default::default())?.select([
            col("Date"),
            col("Code"),
            col("AdjC"),
            col("Va"),
            col("AdjVo"),
            col("news_text"),
        ]));
    }

    let base = concat(shards, UnionArgs::default())?
        .unique(
            Some(vec!["Date".into(), "Code".into()]),
            UniqueKeepStrategy::Last,
        )
        .sort(["Code", "Date"], SortMultipleOptions::default());
    let alpha = alpha_b::compute(alpha_a::compute(base));
    let mut merged = alpha.collect()?;
    let output = "data/processed_market_data.parquet";
    ParquetWriter::new(fs::File::create(output)?).finish(&mut merged)?;
    println!(
        "✅ Merged {} shards into {} ({} rows)",
        RANGES.len(),
        output,
        merged.height()
    );
    Ok(())
}
