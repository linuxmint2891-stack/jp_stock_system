use clap::Parser;
use reqwest::Client;
use std::fs;
use std::io::Write;

#[derive(Parser)]
struct Args {
    /// 取得対象の銘柄コード範囲 (例: 1000-3000)
    #[arg(long)]
    range: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok(); // .env があれば自動読み込み
    let args = Args::parse();

    println!("🔑 Initializing Google Cloud authentication...");

    // 1. OAuth2.0 サービスアカウント認証
    let key = if let Ok(val) = std::env::var("GCP_SA_KEY") {
        yup_oauth2::parse_service_account_key(val)
            .map_err(|e| anyhow::anyhow!("GCP_SA_KEY のパースに失敗しました: {}", e))?
    } else if let Ok(val) = std::env::var("GDRIVE_SECRET_JSON") {
        if val.trim().starts_with('{') {
            yup_oauth2::parse_service_account_key(val)
                .map_err(|e| anyhow::anyhow!("GDRIVE_SECRET_JSON のパースに失敗しました: {}", e))?
        } else {
            let path = std::path::Path::new(&val);
            if !path.exists() {
                anyhow::bail!("GDRIVE_SECRET_JSON で指定されたファイルが見つかりません: {}", val);
            }
            yup_oauth2::read_service_account_key(&val).await?
        }
    } else {
        let default_path = "client_secret.json";
        if !std::path::Path::new(default_path).exists() {
            anyhow::bail!("サービスアカウントキー JSON ファイルが見つかりません。環境変数 GCP_SA_KEY または GDRIVE_SECRET_JSON を設定するか、client_secret.json を用意してください。");
        }
        yup_oauth2::read_service_account_key(default_path).await?
    };

    // トークンキャッシュファイルの作成場所の確認（必要に応じて環境変数からも指定可）
    let cache_path = std::env::var("GDRIVE_TOKEN_CACHE").unwrap_or_else(|_| "tokencache.json".to_string());

    let auth = yup_oauth2::ServiceAccountAuthenticator::builder(key)
        .persist_tokens_to_disk(&cache_path)
        .build()
        .await?;

    // GCS の読み込みに必要なスコープを指定
    let scopes = &["https://www.googleapis.com/auth/devstorage.read_write"];
    let token = auth.token(scopes).await
        .map_err(|e| anyhow::anyhow!("トークンの発行に失敗しました: {}", e))?;
    
    let token_str = token.token().ok_or_else(|| anyhow::anyhow!("アクセストークンの文字列が空です"))?;

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;

    // 2. ダウンロード設定
    let bucket = std::env::var("GCS_BUCKET_NAME").unwrap_or_else(|_| "jp-stock-system-bucket".to_string());
    
    // GCSオブジェクト名の指定（URLエンコード対応）
    let file_name = match &args.range {
        Some(r) => format!("processed_market_data_{}.parquet", r),
        None => "processed_market_data.parquet".to_string(),
    };
    let local_file_name = "processed_market_data.parquet";

    println!("📥 Downloading gs://{}/{} to data/{}...", bucket, file_name, local_file_name);

    // GCSのオブジェクト名に特殊文字が含まれる可能性を考慮してエンコード
    let encoded_file_name = urlencoding::encode(&file_name);
    let url = format!(
        "https://storage.googleapis.com/storage/v1/b/{}/o/{}?alt=media",
        bucket,
        encoded_file_name
    );

    let response = client
        .get(&url)
        .bearer_auth(token_str)
        .send()
        .await?;

    // 3. レスポンス結果のハンドリング
    if !response.status().is_success() {
        let status = response.status();
        let err_body = response.text().await.unwrap_or_default();
        anyhow::bail!(
            "❌ GCS download failed [Status {}]: {}\nURL: gs://{}/{}",
            status,
            err_body,
            bucket,
            file_name
        );
    }

    // 4. ファイル保存
    let bytes = response.bytes().await?;
    fs::create_dir_all("data")?;
    let target_path = format!("data/{}", local_file_name);
    let mut out_file = fs::File::create(&target_path)?;
    out_file.write_all(&bytes)?;
    
    println!("✅ Download successful: {} (size: {} bytes)", target_path, bytes.len());

    Ok(())
}