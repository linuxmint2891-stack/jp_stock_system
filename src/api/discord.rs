// src/api/discord.rs

use serde::Serialize;
use std::env;

#[derive(Serialize)]
struct DiscordWebhookPayload {
    username: String,
    avatar_url: Option<String>,
    embeds: Vec<DiscordEmbed>,
}

#[derive(Serialize)]
struct DiscordEmbed {
    title: String,
    description: String,
    color: u32, // 10進数のカラーコード
    fields: Vec<EmbedField>,
}

#[derive(Serialize)]
struct EmbedField {
    name: String,
    value: String,
    inline: bool,
}

/// 指定したメッセージをDiscordに通知する（環境変数からWebhook URLを取得）
pub async fn notify_discord(ticker: &str, score: f64, reason: &str) -> Result<(), reqwest::Error> {
    // GitHub Secrets またはローカル環境変数からURLを取得
    let webhook_url = match env::var("DISCORD_WEBHOOK_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("[Warning] DISCORD_WEBHOOK_URL is not set. Skipping discord notification.");
            return Ok(());
        }
    };

    // スコアに応じて埋め込みの色を変更（GOなら緑、ガードレール発動なら赤など）
    let color = if score >= 0.70 {
        3066993 // 緑色 (Hex: #2ecc71)
    } else {
        15158332 // 赤色 (Hex: #e74c3c)
    };

    // Discordに送るリッチなペイロードを作成
    let payload = DiscordWebhookPayload {
        username: "株AIスカウトシステム".to_string(),
        avatar_url: None,
        embeds: vec![DiscordEmbed {
            title: format!("🚨 スクリーニング検知: {}", ticker),
            description: "AIによる自動分析結果が完了しました。".to_string(),
            color,
            fields: vec![
                EmbedField {
                    name: "AIスコア".to_string(),
                    value: format!("{:.2}", score),
                    inline: true,
                },
                EmbedField {
                    name: "判定".to_string(),
                    value: if score >= 0.70 { "🟢 GO (買い候補)" } else { "🔴 NO-GO (見送り)" }.to_string(),
                    inline: true,
                },
                EmbedField {
                    name: "分析理由 / 材料".to_string(),
                    value: reason.to_string(),
                    inline: false,
                },
            ],
        }],
    };

    // HTTP POSTリクエストの送信
    let client = reqwest::Client::new();
    let response = client.post(&webhook_url).json(&payload).send().await?;

    if response.status().is_success() {
        println!("[Success] Discord notification sent for {}", ticker);
    } else {
        eprintln!("[Error] Failed to send Discord notification: {:?}", response.status());
    }

    Ok(())
}
