use jp_stock_system::utils::settings::Settings;

#[test]
fn test_settings_load() {
    let settings = Settings::new().expect("Failed to load settings");
    
    // デフォルト値が正しくセットされているか確認
    assert_eq!(settings.data.target_dir, "data");
    assert!(settings.data.parquet_path.contains("processed_market_data.parquet"));
}

#[test]
fn test_settings_jquants_key_is_not_hardcoded() {
    let settings = Settings::new().unwrap();
    // 認証情報は .env / 実行環境からだけ読み込む。未設定なら空文字になる。
    let expected = std::env::var("jquants_api")
        .or_else(|_| std::env::var("JQUANTS_API_KEY"))
        .unwrap_or_default();
    assert_eq!(settings.jquants.api_key, expected);
}
