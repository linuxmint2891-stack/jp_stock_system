use polars::prelude::*;
use std::fs;
use std::path::Path;
use glob::glob;
// use google_drive3::{DriveHub, api::File};
// use yup_oauth2::ServiceAccountAuthenticator;

const PARQUET_PATH: &str = "data/processed_market_data.parquet";

#[tokio::main]
async fn main() -> PolarsResult<()> {
    // --- STEP 1: JSONの集約とParquet化 ---
    println!("🚀 Starting Data Consolidation...");
    
    let mut lazy_dfs = Vec::new();
    let mut processed_files = Vec::new();

    let path_pattern = "./data/daily_*.json"; 
println!("🔍 Searching for files in: {}", path_pattern); // どこを探しているか表示

for entry in glob(path_pattern).expect("Failed to read glob pattern") {
    let path = entry.map_err(|e| PolarsError::ComputeError(e.to_string().into()))?;
    
    // Skip empty or very small files (market closed days)
    let metadata = fs::metadata(&path).map_err(|e| PolarsError::ComputeError(e.to_string().into()))?;
    if metadata.len() < 100 {
        println!("⚠️ Skipping empty file: {:?}", path);
        processed_files.push(path); // Skip but mark as processed to delete later
        continue;
    }

    println!("📖 Found file: {:?}", path);
    
    let file = fs::File::open(&path).map_err(|e| PolarsError::ComputeError(e.to_string().into()))?;
    let df = JsonReader::new(file).finish()?;
    
    // Check if "daily_quotes" column exists before processing
    if df.column("daily_quotes").is_err() {
        println!("⚠️ Skipping file without daily_quotes: {:?}", path);
        processed_files.push(path);
        continue;
    }

    // "daily_quotes" カラムを展開(explode)してネスト解除(unnest)する
    let unnested = df.explode(["daily_quotes"])?.unnest(["daily_quotes"])?;
    lazy_dfs.push(unnested.lazy());

    processed_files.push(path);
}

    if lazy_dfs.is_empty() {
        println!("✨ No new JSON files to process.");
    } else {
        // 既存のParquetがあれば読み込んで結合
        let final_lf = if Path::new(PARQUET_PATH).exists() {
            let existing_lf = LazyFrame::scan_parquet(PARQUET_PATH, Default::default())?;
            concat([existing_lf, concat(lazy_dfs, UnionArgs::default())?], UnionArgs::default())?
        } else {
            concat(lazy_dfs, UnionArgs::default())?
        };

        // 重複排除と保存
        let mut final_df = final_lf
            .unique(Some(vec!["Date".into(), "Code".into()]), UniqueKeepStrategy::Last)
            .collect()?;

        let mut file = fs::File::create(PARQUET_PATH)?;
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
/*
async fn upload_to_gdrive(file_path: &str) -> anyhow::Result<()> {
    // .envファイルを読み込む
    dotenvy::dotenv().ok();
    let folder_id = std::env::var("GDRIVE_FOLDER_ID").expect("GDRIVE_FOLDER_ID must be set in .env");

    // サービスアカウントのキーを読み込む
    let sa_key = yup_oauth2::read_service_account_key("credentials.json").await?;
    let auth = ServiceAccountAuthenticator::builder(sa_key).build().await?;
    
    // hyper 0.14 互換のクライアント作成
    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()?
        .https_or_http()
        .enable_http1()
        .build();
    let client = hyper::Client::builder().build(https);
    
    let hub = DriveHub::new(client, auth);

    let filename = "processed_market_data.parquet";

    // 既存の同名ファイルを検索（指定フォルダ内）
    let query = format!("name = '{}' and trashed = false and '{}' in parents", filename, folder_id);
    let result = hub.files().list()
        .q(&query)
        .supports_all_drives(true)
        .include_items_from_all_drives(true)
        .doit().await?;

        // 検索ロジックを飛ばして、直接IDを指定してみる
let file_id = Some("1dITBY40dMqZsKm_2-s3eHCc_5IVVh8vp".to_string()); 

if let Some(id) = file_id {
    println!("🔄 Forcing update with ID: {}", id);
    // ... updateの処理
}

    let file_id = result.1.files.and_then(|files: Vec<File>| files.get(0).and_then(|f| f.id.clone()));

    let file_content = fs::File::open(file_path)?;

    if let Some(id) = file_id {
        // --- 既存ファイルがある場合は「上書き更新」 ---
        println!("🔄 Existing file found (ID: {}). Updating...", id);
        hub.files().update(File::default(), &id)
            .upload(file_content, "application/octet-stream".parse().unwrap())
            .await?;
    } else {
        // --- ない場合は「新規作成」 ---
        println!("🆕 No existing file found in the folder. Creating new file...");
        let mut f = File::default();
        f.name = Some(filename.to_string());
        f.parents = Some(vec![folder_id]); // ここで親フォルダを指定
        
        hub.files().create(f)
            .upload(file_content, "application/octet-stream".parse().unwrap())
            .await?;
    }
    Ok(())
}
    */