use polars::prelude::*;
use std::fs::File;
use anyhow::Result;

fn main() -> Result<()> {
    let path = "data/processed_market_data.parquet";
    println!("Creating empty parquet file: {}", path);

    // 1. 空の列（Series）を定義
    // 中身は空ですが、データ型（str, f64など）を指定して作成します
    let date = Series::new_empty("Date", &DataType::String);
    let code = Series::new_empty("Code", &DataType::String);
    let close = Series::new_empty("AdjustmentClose", &DataType::Float64);

    // 2. 空のDataFrameを構築
    let mut df = DataFrame::new(vec![date, code, close])?;

    // 3. Parquetファイルとして保存
    let file = File::create(path)?;
    ParquetWriter::new(file).finish(&mut df)?;

    println!("✅ 空のファイルを作成しました。");
    println!("列構成: {:?}", df.get_column_names());
    println!("データ件数: {}", df.height());

    Ok(())
}