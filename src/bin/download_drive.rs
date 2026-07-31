use std::fs;
use std::io::Write;
use reqwest::Client;
use clap::Parser;

#[derive(Parser)]
struct Args {
    /// 取得対象の銘柄コード範囲 (例: 1000-3000)
    #[arg(long)]
    range: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

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
        .timeout(std::time::Duration::from_secs(60))
        .build()?;

    // 2. ダウンロード設定
    let bucket = std::env::var("GCS_BUCKET_NAME").unwrap_or_else(|_| "jp-stock-system-bucket".to_string());
    let file_name = match &args.range {
        Some(r) => format!("processed_market_data_{}.parquet", r),
        None => "processed_market_data.parquet".to_string(),
    };
    let local_file_name = "processed_market_data.parquet";

    println!("📥 Downloading gs://{}/{} to data/{}...", bucket, file_name, local_file_name);

    let url = format!(
        "https://storage.googleapis.com/storage/v1/b/{}/o/{}?alt=media",
        bucket,
        file_name
    );

    let response = client
        .get(&url)
        .bearer_auth(token_str)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let err_body = response.text().await.unwrap_or_default();
        anyhow::bail!("GCS download failed with status {}: {}", status, err_body);
    }

    let bytes = response.bytes().await?;
    fs::create_dir_all("data")?;
    let mut out_file = fs::File::create(format!("data/{}", local_file_name))?;
    out_file.write_all(&bytes)?;
    
    println!("✅ Download successful: data/{}", local_file_name);

    Ok(())
}
