use std::fs;
use google_drive3::{api::File, DriveHub};
use google_drive3::hyper;
use google_drive3::hyper_rustls;
use yup_oauth2::{InstalledFlowAuthenticator, InstalledFlowReturnMethod};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. OAuth2.0 認証 (ユーザーとしてログイン)
    
    // CI環境などで環境変数から直接読み込めるようにする
    // (JSON文字列そのもの、またはファイルパスの両方に対応)
    let secret = if let Ok(val) = std::env::var("GDRIVE_SECRET_JSON") {
        if val.trim().starts_with('{') {
            println!("Using raw JSON from GDRIVE_SECRET_JSON environment variable");
            yup_oauth2::parse_application_secret(val)?
        } else {
            let path = std::path::Path::new(&val);
            if !path.exists() {
                anyhow::bail!("GDRIVE_SECRET_JSON で指定されたファイルが見つかりません: {}", val);
            }
            if std::fs::metadata(path)?.len() == 0 {
                anyhow::bail!("GDRIVE_SECRET_JSON で指定されたファイルが空です: {}", val);
            }
            println!("Reading credentials from file specified in GDRIVE_SECRET_JSON: {}", val);
            yup_oauth2::read_application_secret(val).await?
        }
    } else {
        let default_path = "client_secret.json";
        let path = std::path::Path::new(default_path);
        if !path.exists() {
            anyhow::bail!("client_secret.json が見つかりません。環境変数 GDRIVE_SECRET_JSON を設定するか、ファイルを用意してください。");
        }
        if std::fs::metadata(path)?.len() == 0 {
            anyhow::bail!("client_secret.json が空です。GitHub Secrets の設定を確認してください。");
        }
        yup_oauth2::read_application_secret(default_path)
            .await
            .map_err(|e| anyhow::anyhow!("client_secret.json の読み込みに失敗しました: {}. 形式が正しくない可能性があります。", e))?
    };

    // トークンを保存する場所
    // CI環境などで環境変数からトークンキャッシュを復元できるようにする
    if let Ok(cache_json) = std::env::var("GDRIVE_TOKEN_CACHE") {
        println!("Restoring tokencache.json from environment variable");
        fs::write("tokencache.json", cache_json)?;
    }

    let auth = InstalledFlowAuthenticator::builder(
        secret,
        InstalledFlowReturnMethod::HTTPRedirect,
    ).persist_tokens_to_disk("tokencache.json")
     .build()
     .await?;

    // 🏆 【重要】ここで「ドライブのフル権限」を強制的に指定します
    let scopes = &["https://www.googleapis.com/auth/drive"];
    
    // 認証の試行。キャッシュがない場合は対話型になるためCIでは失敗する。
    auth.token(scopes).await.map_err(|e| {
        anyhow::anyhow!("認証に失敗しました: {}. CI環境の場合は tokencache.json が正しく提供されているか確認してください。", e)
    })?;

    let connector = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .expect("Native roots could not be loaded")
        .https_or_http()
        .enable_http1()
        .build();

    let client = hyper::Client::builder().build(connector);
    let hub = DriveHub::new(client, auth);

    // 2. アップロード設定
    let file_path = "data/processed_market_data.parquet";
    let file_name = "processed_market_data.parquet";

    if !std::path::Path::new(file_path).exists() {
        println!("⚠️ アップロード対象ファイルが見つかりません: {}", file_path);
        return Ok(());
    }

    // 自分のストレージ状況を確認
    let (_, about) = hub.about().get().add_scope(google_drive3::api::Scope::Full)
        .param("fields", "storageQuota")
        .doit().await?;
    
    if let Some(quota) = about.storage_quota.as_ref() {
        println!("📊 ストレージ状況: 使用量 {} / 全容量 {}", 
            quota.usage.unwrap_or(0),
            quota.limit.unwrap_or(0)
        );
    }

    // 3. 既存ファイルの検索
    let query = format!("name = '{}' and trashed = false", file_name);
    let (_, file_list) = hub.files().list().q(&query)
        .add_scope(google_drive3::api::Scope::Full)
        .doit().await?;
    
    let existing_file_id = file_list.files.and_then(|f| f.get(0).and_then(|f| f.id.clone()));
    let file_data = fs::File::open(file_path)?;

    match existing_file_id {
        Some(id) => {
            println!("🔄 上書きアップロード中 (ID: {})...", id);
            hub.files().update(File::default(), &id)
                .add_scope(google_drive3::api::Scope::Full)
                .upload(file_data, "application/octet-stream".parse().unwrap())
                .await?;
            println!("✅ 上書き成功！");
        },
        None => {
            println!("🆕 新規アップロード中...");
            let mut file_meta = File::default();
            file_meta.name = Some(file_name.to_string());
            hub.files().create(file_meta)
                .add_scope(google_drive3::api::Scope::Full)
                .upload(file_data, "application/octet-stream".parse().unwrap())
                .await?;
            println!("✅ 新規作成成功！");
        }
    }

    Ok(())
}