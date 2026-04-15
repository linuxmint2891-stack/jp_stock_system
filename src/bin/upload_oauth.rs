use std::fs;
use google_drive3::{api::File, DriveHub};
use google_drive3::hyper;
use google_drive3::hyper_rustls;
use yup_oauth2::{InstalledFlowAuthenticator, InstalledFlowReturnMethod};

#[tokio::main]
async fn main() {
    // 1. OAuth2.0 認証 (ユーザーとしてログイン)
    let secret = yup_oauth2::read_application_secret("client_secret.json")
        .await
        .expect("client_secret.jsonが見つかりません");

    // トークンを保存する場所 (次回からブラウザログイン不要にするため)
    let auth = InstalledFlowAuthenticator::builder(
        secret,
        InstalledFlowReturnMethod::HTTPRedirect,
    ).persist_tokens_to_disk("tokencache.json")
     .build()
     .await
     .unwrap();

    // 🏆 【重要】ここで「ドライブのフル権限」を強制的に指定します
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

    // 2. アップロード設定
    let file_path = "data/test.json";
    let file_name = "test.json";

    // 自分のストレージ状況を確認 (今度は 15GB などの数値が出るはず)
    let (_, about) = hub.about().get().add_scope(google_drive3::api::Scope::Full)
        .param("fields", "storageQuota")
        .doit().await.unwrap();
    if let Some(quota) = about.storage_quota.as_ref() {
        println!("📊 あなたのストレージ状況: 使用量 {} / 全容量 {}", 
            quota.usage.unwrap_or(0),
            quota.limit.unwrap_or(0)
        );
    }

    // 3. 既存ファイルの検索
    let query = format!("name = '{}' and trashed = false", file_name);
    let (_, file_list) = hub.files().list().q(&query)
        .add_scope(google_drive3::api::Scope::Full)
        .doit().await.unwrap();
    let existing_file_id = file_list.files.and_then(|f| f.get(0).and_then(|f| f.id.clone()));

    let file_data = fs::File::open(file_path).expect("data/test.json を用意してください");

    match existing_file_id {
        Some(id) => {
            println!("🔄 自分の容量を使って上書きします (ID: {})...", id);
            hub.files().update(File::default(), &id)
                .add_scope(google_drive3::api::Scope::Full)
                .upload(file_data, "application/json".parse().unwrap())
                .await.expect("上書き失敗");
            println!("✅ 上書き成功！");
        },
        None => {
            println!("🆕 自分の容量を使って新規作成します...");
            let mut file_meta = File::default();
            file_meta.name = Some(file_name.to_string());
            hub.files().create(file_meta)
                .add_scope(google_drive3::api::Scope::Full)
                .upload(file_data, "application/json".parse().unwrap())
                .await.expect("作成失敗");
            println!("✅ 新規作成成功！");
        }
    }
}