use clap::Parser;
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

    /// Drive上の任意のファイル名（rangeとは併用不可）
    #[arg(long, conflicts_with = "range")]
    file_name: Option<String>,

    /// 保存先のローカルパス（--file-name指定時のみ有効）
    #[arg(long, requires = "file_name")]
    local_path: Option<String>,

    /// ファイルがまだ存在しない場合も成功として扱う
    #[arg(long)]
    allow_missing: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🔑 Initializing Google Drive service-account authentication...");
    let args = Args::parse();
    let (file_name, local_path) = file_spec(&args)?;
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

    let Some(file_id) = file_id else {
        if args.allow_missing {
            println!("ℹ️ Google Drive に {} はまだ存在しません。", file_name);
            return Ok(());
        }
        anyhow::bail!(
            "Google Drive に {} が見つかりません。GDRIVE_UPLOAD_FOLDER_ID とフォルダ共有設定を確認してください。",
            file_name
        );
    };

    println!("📥 Downloading Google Drive file {}...", file_name);
    let (mut response, _) = hub
        .files()
        .get(&file_id)
        .param("alt", "media")
        .add_scope(google_drive3::api::Scope::Full)
        .doit()
        .await?;
    let bytes = hyper::body::to_bytes(response.body_mut()).await?;

    if let Some(parent) = Path::new(&local_path).parent() {
        fs::create_dir_all(parent)?;
    }
    let mut output = fs::File::create(&local_path)?;
    output.write_all(&bytes)?;
    println!("✅ Downloaded {} bytes to {}", bytes.len(), local_path);
    Ok(())
}

fn file_spec(args: &Args) -> anyhow::Result<(String, String)> {
    if let Some(file_name) = &args.file_name {
        let local_path = args
            .local_path
            .clone()
            .unwrap_or_else(|| format!("data/{file_name}"));
        return Ok((file_name.clone(), local_path));
    }
    let file_name = match &args.range {
        Some(range) => format!("processed_market_data_{range}.parquet"),
        None => "processed_market_data.parquet".to_owned(),
    };
    Ok((file_name.clone(), format!("data/{file_name}")))
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
