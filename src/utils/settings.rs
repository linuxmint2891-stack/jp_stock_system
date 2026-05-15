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
            .set_default("jquants.api_key", "-fMC9EnlXau-2iA_I3xk6cyZxAI_xZutVBNVeht3VsU")?
            .set_default("data.target_dir", "data")?
            .set_default("data.parquet_path", "data/processed_market_data.parquet")?
            .set_default("data.min_valid_size", 819200)? // 800KB
            // Load from file
            .add_source(File::with_name("settings").required(false))
            // Load from environment variables (e.g. JQUANTS_API_KEY)
            .set_override_option("jquants.api_key", env::var("JQUANTS_API_KEY").ok())?
            .build();

        s.and_then(|config| config.try_deserialize())
    }
}
