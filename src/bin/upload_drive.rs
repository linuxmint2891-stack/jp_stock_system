use std::fs;
use reqwest::Client;
use clap::Parser;

#[derive(Parser)]
struct Args {
    /// アップロード対象の銘柄コード範囲 (例: 1000-3000)
    #[arg(long)]
    range: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let file_path = "data/processed_market_data.parquet";
    if !std::path::Path::new(file_path).exists() {
        println!("⚠️ アップロード対象ファイルが見つかりません: {}", file_path);
        return Ok(());
    }

    // 1. OAuth2.0 サービスアカウント認証
    let key = if let Ok(val) = std::env::var("GCP_SA_KEY") {
        yup_oauth2::parse_service_account_key(val)?
    } else if let Ok(val) = std::env::var("GDRIVE_SECRET_JSON") {
        if val.trim().starts_with('{') {
            yup_oauth2::parse_service_account_key(val)?
        } else {
            yup_oauth2::read_service_account_key(val).await?
        }
    } else {
        let default_path = "client_secret.json";
        if !std::path::Path::new(default_path).exists() {
            anyhow::bail!("サービスアカウントキー JSON ファイルが見つかりません。環境変数 GCP_SA_KEY または GDRIVE_SECRET_JSON を設定するか、client_secret.json を用意してください。");
        }
        yup_oauth2::read_service_account_key(default_path).await?
    };

    let auth = yup_oauth2::ServiceAccountAuthenticator::builder(key)
        .persist_tokens_to_disk("tokencache.json")
        .build()
        .await?;

    let scopes = &["https://www.googleapis.com/auth/devstorage.read_write"];
    let token = auth.token(scopes).await?;
    let token_str = token.token().ok_or_else(|| anyhow::anyhow!("アクセストークンの取得に失敗しました"))?;

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    // 2. アップロード設定
    let bucket = std::env::var("GCS_BUCKET_NAME").unwrap_or_else(|_| "jp-stock-system-bucket".to_string());
    let file_name = match &args.range {
        Some(r) => format!("processed_market_data_{}.parquet", r),
        None => "processed_market_data.parquet".to_string(),
    };

    println!("📤 Uploading data/processed_market_data.parquet to gs://{}/{}...", bucket, file_name);

    let url = format!(
        "https://storage.googleapis.com/upload/storage/v1/b/{}/o?uploadType=media&name={}",
        bucket,
        file_name
    );

    let file_data = fs::read(file_path)?;

    let response = client
        .post(&url)
        .bearer_auth(token_str)
        .header("Content-Type", "application/octet-stream")
        .body(file_data)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let err_body = response.text().await.unwrap_or_default();
        anyhow::bail!("GCS upload failed with status {}: {}", status, err_body);
    }

    println!("✅ Upload successful!");

    Ok(())
}