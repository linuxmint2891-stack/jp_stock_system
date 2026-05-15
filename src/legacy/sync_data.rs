use polars::prelude::*;
use std::fs;
use std::path::Path;
use glob::glob;
use jp_stock_system::utils::settings::Settings;
use jp_stock_system::alpha::{alpha_a, alpha_b};

#[tokio::main]
async fn main() -> PolarsResult<()> {
    let settings = Settings::new().map_err(|e| PolarsError::ComputeError(e.to_string().into()))?;
    let parquet_path = &settings.data.parquet_path;
    let target_dir = &settings.data.target_dir;

    // --- STEP 1: JSONの集約とParquet化 ---
    println!("🚀 Starting Data Consolidation...");
    
    let mut lazy_dfs = Vec::new();
    let mut processed_files = Vec::new();

    let path_pattern = format!("{}//daily_*.json", target_dir); 
    println!("🔍 Searching for files in: {}", path_pattern);

    for entry in glob(&path_pattern).expect("Failed to read glob pattern") {
        let path = entry.map_err(|e| PolarsError::ComputeError(e.to_string().into()))?;
        
        let metadata = fs::metadata(&path).map_err(|e| PolarsError::ComputeError(e.to_string().into()))?;
        if metadata.len() < 100 {
            println!("⚠️ Skipping empty file: {:?}", path);
            processed_files.push(path);
            continue;
        }

        println!("📖 Found file: {:?}", path);
        
        let file = fs::File::open(&path).map_err(|e| PolarsError::ComputeError(e.to_string().into()))?;
        let df = JsonReader::new(file).finish()?;
        
        if df.column("daily_quotes").is_err() {
            println!("⚠️ Skipping file without daily_quotes: {:?}", path);
            processed_files.push(path);
            continue;
        }

        // "daily_quotes" カラムを展開してネスト解除
        let unnested = df.explode(["daily_quotes"])?.unnest(["daily_quotes"])?;
        
        // 既存の Parquet スキーマに合わせるための選択
        // 必要なカラム: Date, Code, AdjC, Va, AdjVo
        // また、news_text が無い場合は空文字で追加する
        let selected = unnested.lazy().select([
            col("Date"),
            col("Code"),
            col("AdjC"),
            col("Va"),
            col("AdjVo"),
            lit("").alias("news_text")
        ]);

        lazy_dfs.push(selected);
        processed_files.push(path);
    }

    if lazy_dfs.is_empty() {
        println!("✨ No new JSON files to process.");
    } else {
        // 既存のParquetがあれば読み込んで結合
        let final_lf = if Path::new(parquet_path).exists() {
            let existing_lf = LazyFrame::scan_parquet(parquet_path, Default::default())?;
            
            // カラム順序を統一して結合
            let new_data_lf = concat(lazy_dfs, UnionArgs::default())?;
            concat([existing_lf, new_data_lf], UnionArgs::default())?
        } else {
            concat(lazy_dfs, UnionArgs::default())?
        };

        // 重複排除とソート
        let combined_lf = final_lf
            .unique(Some(vec!["Date".into(), "Code".into()]), UniqueKeepStrategy::Last)
            .sort(["Code", "Date"], SortMultipleOptions::default());

        // Alpha 再計算 (データの整合性を保つため)
        println!("🧪 Computing Alphas...");
        let alpha_lf = alpha_a::compute(combined_lf);
        let alpha_lf = alpha_b::compute(alpha_lf);

        let mut final_df = alpha_lf.collect()?;

        let mut file = fs::File::create(parquet_path)?;
        ParquetWriter::new(&mut file).finish(&mut final_df)?;
        println!("✅ Parquet updated. Total rows: {}", final_df.height());

        // --- STEP 2: 使用済みJSONの一括削除 ---
        for path in processed_files {
            fs::remove_file(&path).map_err(|e| PolarsError::ComputeError(e.to_string().into()))?;
        }
        println!("🗑️ All processed JSON files deleted. Storage cleared!");
    }

    Ok(())
}
