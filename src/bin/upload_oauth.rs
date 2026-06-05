use std::fs;
use std::path::Path;
use google_drive3::{api::File, DriveHub};
use google_drive3::hyper;
use google_drive3::hyper_rustls;
use yup_oauth2::{InstalledFlowAuthenticator, InstalledFlowReturnMethod};
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 引数の処理
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Usage: cargo run --release --bin upload_oauth <file_path>");
        println!("Example: cargo run --release --bin upload_oauth data/processed_market_data.parquet");
        return Ok(());
    }
    let file_path_str = &args[1];
    let file_path = Path::new(file_path_str);
    
    if !file_path.exists() {
        anyhow::bail!("File not found: {}", file_path_str);
    }
    
    let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("upload_file");

    // 2. OAuth2.0 認証
    let secret = yup_oauth2::read_application_secret("client_secret.json")
        .await
        .expect("client_secret.jsonが見つかりません");

    let auth = InstalledFlowAuthenticator::builder(
        secret,
        InstalledFlowReturnMethod::HTTPRedirect,
    ).persist_tokens_to_disk("tokencache.json")
     .build()
     .await
     .unwrap();

    let scopes = &["https://www.googleapis.com/auth/drive"];
    auth.token(scopes).await.expect("認証に失敗しました");

    let connector = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .expect("Native roots could not be loaded")
        .https_or_http()
        .enable_http1()
        .build();

    let client = hyper::Client::builder().build(connector);
    let hub = DriveHub::new(client, auth);

    // 3. 既存ファイルの検索 (同名のファイルがあれば上書きするため)
    let query = format!("name = '{}' and trashed = false", file_name);
    let (_, file_list) = hub.files().list().q(&query)
        .add_scope(google_drive3::api::Scope::Full)
        .doit().await.unwrap();
    
    let existing_file_id = file_list.files.and_then(|f| f.get(0).and_then(|f| f.id.clone()));
    let file_data = fs::File::open(file_path)?;

    // MIMEタイプの決定 (Parquetなら binary、それ以外は適宜)
    let mime_type = if file_name.ends_with(".parquet") {
        "application/octet-stream"
    } else if file_name.ends_with(".json") {
        "application/json"
    } else {
        "text/plain"
    };

    match existing_file_id {
        Some(id) => {
            println!("🔄 上書きアップロード中 (ID: {}, File: {})...", id, file_name);
            hub.files().update(File::default(), &id)
                .add_scope(google_drive3::api::Scope::Full)
                .upload(file_data, mime_type.parse().unwrap())
                .await?;
            println!("✅ 上書き成功！");
        },
        None => {
            println!("🆕 新規アップロード中 (File: {})...", file_name);
            let mut file_meta = File::default();
            file_meta.name = Some(file_name.to_string());
            hub.files().create(file_meta)
                .add_scope(google_drive3::api::Scope::Full)
                .upload(file_data, mime_type.parse().unwrap())
                .await?;
            println!("✅ 新規作成成功！");
        }
    }

    Ok(())
}
