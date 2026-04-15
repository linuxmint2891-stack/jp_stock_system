use google_drive3::DriveHub;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let folder_id = std::env::var("GDRIVE_FOLDER_ID").expect("GDRIVE_FOLDER_ID not set");

    println!("Authenticating...");
    let sa_key = yup_oauth2::read_service_account_key("credentials.json").await?;
    let auth = yup_oauth2::ServiceAccountAuthenticator::builder(sa_key).build().await?;
    let hub = DriveHub::new(
        hyper::Client::builder().build(
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_native_roots()?
                .https_or_http()
                .enable_http1()
                .build()
        ), 
        auth
    );

    println!("Target Folder ID: {}", folder_id);

    println!("--- Testing Folder Access ---");
    let folder_res = hub.files().get(&folder_id)
        .supports_all_drives(true)
        .doit().await;

    match folder_res {
        Ok((_, f)) => println!("✅ Folder found: {} (Kind: {:?})", f.name.unwrap_or_default(), f.kind),
        Err(e) => println!("❌ Cannot access folder: {:?}", e),
    }

    println!("--- Searching for files in folder ---");
    let query = format!("'{}' in parents and trashed = false", folder_id);
    let result = hub.files().list()
        .q(&query)
        .corpora("allDrives") // 共有ドライブ全体を検索対象にする
        .supports_all_drives(true)
        .include_items_from_all_drives(true)
        .doit().await?;

    let files = result.1.files.unwrap_or_default();
    println!("Found {} items in folder.", files.len());
    for file in files {
        println!("- Name: {}, ID: {}, MimeType: {}", 
                 file.name.unwrap_or_default(), 
                 file.id.unwrap_or_default(),
                 file.mime_type.unwrap_or_default());
    }

    Ok(())
}
