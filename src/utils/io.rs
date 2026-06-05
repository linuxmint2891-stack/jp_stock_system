use polars::prelude::*;
use std::fs::File;

pub fn save_parquet(df: &mut DataFrame, path: &str) -> PolarsResult<()> {
    let file = File::create(path).map_err(|e| {
        // String に変換してから ComputeError に渡す
        PolarsError::ComputeError(e.to_string().into())
    })?;
    
    // ParquetWriter は明示的にこのパスから使うか、features確認が必要
    ParquetWriter::new(file).finish(df)?;
    Ok(())
}

pub fn load_parquet(path: &str) -> PolarsResult<DataFrame> {
    let file = File::open(path).map_err(|e| {
        PolarsError::ComputeError(e.to_string().into())
    })?;
    
    ParquetReader::new(file).finish()
}