use clap::Parser;
use polars::prelude::*;
use std::fs;

#[derive(Parser)]
struct Args {
    /// 銘柄コードの範囲（例: 1000-3000）
    #[arg(long)]
    range: String,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let (start, end) = parse_range(&args.range)?;
    let input = "data/processed_market_data.parquet";
    let output = format!("data/processed_market_data_{}.parquet", args.range);

    let mut shard = LazyFrame::scan_parquet(input, Default::default())?
        .filter(
            col("Code")
                .cast(DataType::String)
                .str()
                .slice(lit(0), lit(4))
                .gt_eq(lit(format!("{start:04}")))
                .and(
                    col("Code")
                        .cast(DataType::String)
                        .str()
                        .slice(lit(0), lit(4))
                        .lt_eq(lit(format!("{end:04}"))),
                ),
        )
        .collect()?;
    ParquetWriter::new(fs::File::create(&output)?).finish(&mut shard)?;
    println!("✅ Created {} ({} rows)", output, shard.height());
    Ok(())
}

fn parse_range(range: &str) -> anyhow::Result<(u32, u32)> {
    let (start, end) = range
        .split_once('-')
        .ok_or_else(|| anyhow::anyhow!("範囲指定の形式が不正です: {range}"))?;
    let start = start.parse()?;
    let end = end.parse()?;
    if start > end {
        anyhow::bail!("範囲指定の開始値が終了値を超えています: {range}");
    }
    Ok((start, end))
}
