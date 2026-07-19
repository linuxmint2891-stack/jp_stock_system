pub mod io;
pub mod settings;

use anyhow::Result;
use polars::prelude::*;

/// 保存済みParquetから、Yahoo照会に使う重複なしの銘柄コードを取得する。
pub fn get_unique_codes(parquet_path: &str) -> Result<Vec<String>> {
    let codes = LazyFrame::scan_parquet(parquet_path, Default::default())?
        .select([col("Code").cast(DataType::String).unique()])
        .collect()?
        .column("Code")?
        .str()?
        .into_iter()
        .flatten()
        .map(str::to_owned)
        .collect();

    Ok(codes)
}
