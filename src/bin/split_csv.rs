use polars::prelude::*;
use std::fs;

fn main() -> PolarsResult<()> {
    let input_path = "/home/michiharu/ダウンロード/processed_market_data.parquet";
    let output_dir = "/home/michiharu/ダウンロード/split_data";
    let chunk_size = 200_000;

    // 出力ディレクトリの作成
    fs::create_dir_all(output_dir).map_err(|e| PolarsError::ComputeError(e.to_string().into()))?;

    println!("📂 Reading Parquet file...");
    let df = LazyFrame::scan_parquet(input_path, Default::default())?.collect()?;
    let total_rows = df.height();
    let num_chunks = (total_rows as f64 / chunk_size as f64).ceil() as usize;

    println!("✂️ Splitting {} rows into {} chunks...", total_rows, num_chunks);

    for i in 0..num_chunks {
        let start = i * chunk_size;
        let end = ((i + 1) * chunk_size).min(total_rows);
        
        // データのスライスを取得
        let mut chunk_df = df.slice(start as i64, end - start);
        
        let output_path = format!("{}/part_{:02}.csv", output_dir, i + 1);
        println!("💾 Saving {}...", output_path);
        
        let file = fs::File::create(&output_path).map_err(|e| PolarsError::ComputeError(e.to_string().into()))?;
        CsvWriter::new(file).finish(&mut chunk_df)?;
    }

    println!("✅ Successfully split into {} files in {}", num_chunks, output_dir);
    Ok(())
}