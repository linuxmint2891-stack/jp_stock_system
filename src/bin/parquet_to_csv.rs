use polars::prelude::*;
use std::fs::File;

fn main() -> PolarsResult<()> {
    let input_path = "/home/michiharu/ダウンロード/processed_market_data.parquet";
    let output_path = "/home/michiharu/ダウンロード/processed_market_data.csv";

    println!("📂 Reading Parquet file...");
    let mut df = LazyFrame::scan_parquet(input_path, Default::default())?.collect()?;

    println!("💾 Writing to CSV (this may take a moment)...");
    let file = File::create(output_path).map_err(|e| PolarsError::ComputeError(e.to_string().into()))?;
    CsvWriter::new(file).finish(&mut df)?;

    println!("✅ Successfully converted to: {}", output_path);
    println!("📈 Total rows converted: {}", df.height());

    Ok(())
}