use serde::Deserialize;
use config::{Config, ConfigError, File};
use std::env;
use std::collections::HashMap;

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
pub struct TradingSettings {
    pub broker: String,
    pub execution_mode: String,
    pub account_type: String,
    pub paper_total_budget: u64,
    pub paper_position_budget: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SymbolExitOverride {
    pub stop_loss_percent: Option<f64>,
    pub take_profit_percent: Option<f64>,
    pub trailing_stop_percent: Option<f64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ExitStrategySettings {
    pub atr_lookback_days: usize,
    pub stop_loss_atr_multiplier: f64,
    pub take_profit_atr_multiplier: f64,
    pub trailing_stop_atr_multiplier: f64,
    pub max_stop_loss_percent: f64,
    pub fallback_atr_percent: f64,
    #[serde(default)]
    pub overrides: HashMap<String, SymbolExitOverride>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub jquants: JQuantsSettings,
    pub data: DataSettings,
    pub trading: TradingSettings,
    pub exit_strategy: ExitStrategySettings,
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
            .set_default("trading.broker", "moomoo証券")?
            .set_default("trading.execution_mode", "manual_fractional")?
            .set_default("trading.account_type", "specified_only")?
            .set_default("trading.paper_total_budget", 100_000_u64)?
            .set_default("trading.paper_position_budget", 10_000_u64)?
            .set_default("exit_strategy.atr_lookback_days", 14)?
            .set_default("exit_strategy.stop_loss_atr_multiplier", 1.5)?
            .set_default("exit_strategy.take_profit_atr_multiplier", 2.5)?
            .set_default("exit_strategy.trailing_stop_atr_multiplier", 1.5)?
            .set_default("exit_strategy.max_stop_loss_percent", 15.0)?
            .set_default("exit_strategy.fallback_atr_percent", 5.0)?
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
