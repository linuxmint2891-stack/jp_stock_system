use serde::Deserialize;
use config::{Config, ConfigError, File};
use std::env;

#[derive(Debug, Deserialize, Clone)]
pub struct JQuantsSettings {
    pub api_key: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DataSettings {
    pub target_dir: String,
    pub parquet_path: String,
    pub min_valid_size: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub jquants: JQuantsSettings,
    pub data: DataSettings,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();

        let s = Config::builder()
            // Add default values
            // APIキーはリポジトリに保存しない。`.env` の `jquants_api` を優先して読む。
            .set_default("jquants.api_key", "")?
            .set_default("data.target_dir", "data")?
            .set_default("data.parquet_path", "data/processed_market_data.parquet")?
            .set_default("data.min_valid_size", 819200)? // 800KB
            // Load from file
            .add_source(File::with_name("settings").required(false))
            // 既存環境との互換のため大文字名も受け付ける。
            .set_override_option(
                "jquants.api_key",
                env::var("jquants_api")
                    .ok()
                    .or_else(|| env::var("JQUANTS_API_KEY").ok()),
            )?
            .build();

        s.and_then(|config| config.try_deserialize())
    }
}

#[cfg(test)]
mod tests {
    use super::Settings;

    #[test]
    fn reads_jquants_key_from_environment_only() {
        let settings = Settings::new().unwrap();
        let expected = std::env::var("jquants_api")
            .or_else(|_| std::env::var("JQUANTS_API_KEY"))
            .unwrap_or_default();
        assert_eq!(settings.jquants.api_key, expected);
    }
}
