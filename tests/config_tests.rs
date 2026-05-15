use jp_stock_system::utils::settings::Settings;

#[test]
fn test_settings_load() {
    let settings = Settings::new().expect("Failed to load settings");
    
    // デフォルト値が正しくセットされているか確認
    assert_eq!(settings.data.target_dir, "data");
    assert!(settings.data.parquet_path.contains("processed_market_data.parquet"));
}

#[test]
fn test_settings_jquants_key_exists() {
    let settings = Settings::new().unwrap();
    // デフォルト値または環境変数が読み込まれているはず
    assert!(!settings.jquants.api_key.is_empty());
}
