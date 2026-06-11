use std::fs;
use std::io::Write;
use google_drive3::DriveHub;
use google_drive3::hyper;
use google_drive3::hyper_rustls;
use yup_oauth2::{InstalledFlowAuthenticator, InstalledFlowReturnMethod};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. OAuth2.0 認証
    let secret = if let Ok(json) = std::env::var("GDRIVE_SECRET_JSON") {
        yup_oauth2::parse_application_secret(json)?
    } else {
        yup_oauth2::read_application_secret("client_secret.json").await?
    };

    if let Ok(cache_json) = std::env::var("GDRIVE_TOKEN_CACHE") {
        fs::write("tokencache.json", cache_json)?;
    }

    let auth = InstalledFlowAuthenticator::builder(
        secret,
        InstalledFlowReturnMethod::HTTPRedirect,
    ).persist_tokens_to_disk("tokencache.json")
     .build()
     .await?;

    let connector = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .expect("Native roots could not be loaded")
        .https_or_http()
        .enable_http1()
        .build();

    let client = hyper::Client::builder().build(connector);
    let hub = DriveHub::new(client, auth);

    // 2. ファイルの検索
    let file_name = "processed_market_data.parquet";
    let query = format!("name = '{}' and trashed = false", file_name);
    let (_, file_list) = hub.files().list().q(&query)
        .add_scope(google_drive3::api::Scope::Full)
        .doit().await?;

    let file_id = file_list.files.and_then(|f| f.get(0).and_then(|f| f.id.clone()));

    match file_id {
        Some(id) => {
            println!("📥 Downloading {} (ID: {})...", file_name, id);
            let (mut response, _) = hub.files().get(&id)
                .add_scope(google_drive3::api::Scope::Full)
                .param("alt", "media")
                .doit().await?;

            let mut out_file = fs::File::create(format!("data/{}", file_name))?;
            // Note: google_drive3 5.0 uses hyper for response body
            // We need to read the body bytes
            let body_bytes = hyper::body::to_bytes(response.body_mut()).await?;
            out_file.write_all(&body_bytes)?;
            
            println!("✅ Download successful: data/{}", file_name);
        },
        None => {
            println!("⚠️ File not found on Google Drive: {}", file_name);
        }
    }

    Ok(())
}
