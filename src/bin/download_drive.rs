use google_drive3::hyper;
use google_drive3::hyper_rustls;
use google_drive3::DriveHub;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Parser)]
struct Args {
    /// 銘柄範囲ごとのParquetを指定（例: 1000-3000）
    #[arg(long)]
    range: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🔑 Initializing Google Drive service-account authentication...");
    let args = Args::parse();
    let (file_name, local_path) = parquet_names(args.range.as_deref());
    let auth = yup_oauth2::ServiceAccountAuthenticator::builder(load_service_account_key().await?)
        .build()
        .await?;
    let hub = DriveHub::new(drive_client(), auth);

    let folder_id = std::env::var("GDRIVE_UPLOAD_FOLDER_ID").ok();
    let query = drive_file_query(&file_name, folder_id.as_deref());
    let (_, file_list) = hub
        .files()
        .list()
        .q(&query)
        .add_scope(google_drive3::api::Scope::Full)
        .doit()
        .await?;
    let file_id = file_list
        .files
        .and_then(|files| files.into_iter().next())
        .and_then(|file| file.id);

    let file_id = file_id.ok_or_else(|| {
        anyhow::anyhow!(
            "Google Drive に {} が見つかりません。GDRIVE_UPLOAD_FOLDER_ID とフォルダ共有設定を確認してください。",
            file_name
        )
    })?;

    println!("📥 Downloading Google Drive file {}...", file_name);
    let (mut response, _) = hub
        .files()
        .get(&file_id)
        .param("alt", "media")
        .add_scope(google_drive3::api::Scope::Full)
        .doit()
        .await?;
    let bytes = hyper::body::to_bytes(response.body_mut()).await?;

    fs::create_dir_all("data")?;
    let mut output = fs::File::create(&local_path)?;
    output.write_all(&bytes)?;
    println!("✅ Downloaded {} bytes to {}", bytes.len(), local_path);
    Ok(())
}

fn parquet_names(range: Option<&str>) -> (String, String) {
    let file_name = match range {
        Some(range) => format!("processed_market_data_{range}.parquet"),
        None => "processed_market_data.parquet".to_owned(),
    };
    (file_name.clone(), format!("data/{file_name}"))
}

fn drive_client() -> hyper::Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>> {
    let connector = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .expect("Native roots could not be loaded")
        .https_or_http()
        .enable_http1()
        .build();
    hyper::Client::builder().build(connector)
}

fn drive_file_query(file_name: &str, folder_id: Option<&str>) -> String {
    match folder_id {
        Some(folder_id) => format!(
            "name = '{}' and '{}' in parents and trashed = false",
            file_name, folder_id
        ),
        None => format!("name = '{}' and trashed = false", file_name),
    }
}

async fn load_service_account_key() -> anyhow::Result<yup_oauth2::ServiceAccountKey> {
    for variable in ["GCP_SA_KEY", "GDRIVE_SECRET_JSON"] {
        if let Ok(value) = std::env::var(variable) {
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            if value.starts_with('{') {
                return yup_oauth2::parse_service_account_key(value.to_owned()).map_err(|e| {
                    anyhow::anyhow!("{variable} はサービスアカウント鍵として不正です: {e}")
                });
            }
            let path = Path::new(value);
            if !path.exists() {
                anyhow::bail!("{variable} で指定されたファイルが見つかりません: {value}");
            }
            return yup_oauth2::read_service_account_key(path)
                .await
                .map_err(|e| anyhow::anyhow!("{variable} の読み込みに失敗しました: {e}"));
        }
    }

    for path in [
        PathBuf::from("data/API_Key/credentials.json"),
        PathBuf::from("credentials.json"),
    ] {
        if path.exists() {
            return yup_oauth2::read_service_account_key(&path)
                .await
                .map_err(|e| {
                    anyhow::anyhow!(
                        "サービスアカウント鍵 {} の読み込みに失敗しました: {e}",
                        path.display()
                    )
                });
        }
    }
    anyhow::bail!("Google Drive のサービスアカウント鍵が見つかりません")
}
use clap::Parser;
