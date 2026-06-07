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

/// 💥 決済（トレーリングストップ・絶対損切り）が発動したことをDiscordに通知する
pub async fn notify_trade_exit(
    code: &str,
    name: &str,
    entry_price: f64,
    exit_price: f64,
    pl_pct: f64,
    reason: &str,
) -> Result<(), reqwest::Error> {
    let webhook_url = match env::var("DISCORD_WEBHOOK_URL") {
        Ok(url) => url,
        Err(_) => return Ok(()),
    };

    // 利益が出たか損切りかによって色を分ける
    let color = if pl_pct >= 0.0 {
        3447003 // 青色系 (Hex: #3498db)
    } else {
        15158332 // 赤色系 (Hex: #e74c3c)
    };

    let payload = DiscordWebhookPayload {
        username: "株AIスカウトシステム".to_string(),
        avatar_url: None,
        embeds: vec![DiscordEmbed {
            title: format!("💥 ポジション決済実行: {} ({})", name, code),
            description: format!("設定された売買ルールに基づき、自動決済されました。\n**決済理由: {}**", reason),
            color,
            fields: vec![
                EmbedField {
                    name: "購入価格".to_string(),
                    value: format!("{:.1} 円", entry_price),
                    inline: true,
                },
                EmbedField {
                    name: "決済価格".to_string(),
                    value: format!("{:.1} 円", exit_price),
                    inline: true,
                },
                EmbedField {
                    name: "最終損益率".to_string(),
                    value: format!("**{}{:.2} %**", if pl_pct >= 0.0 { "+" } else { "" }, pl_pct),
                    inline: false,
                },
            ],
        }],
    };

    let client = reqwest::Client::new();
    client.post(&webhook_url).json(&payload).send().await?;
    Ok(())
}

/// 📊 現在の通算勝率と累計成績をDiscordに報告する
pub async fn notify_performance_report(
    total_trades: i64,
    win_trades: i64,
    win_rate: f64,
    total_pl: f64,
) -> Result<(), reqwest::Error> {
    let webhook_url = match env::var("DISCORD_WEBHOOK_URL") {
        Ok(url) => url,
        Err(_) => return Ok(()),
    };

    // トータル損益がプラスなら金色、マイナスならグレー
    let color = if total_pl >= 0.0 {
        15844367 // 金色 (Hex: #f1c40f)
    } else {
        9807270  // グレー (Hex: #95a5a6)
    };

    let payload = DiscordWebhookPayload {
        username: "株AIスカウトシステム".to_string(),
        avatar_url: None,
        embeds: vec![DiscordEmbed {
            title: "📊 Gemma 3 スクリーニング通算成績レポート".to_string(),
            description: "これまでのペーパートレードの運用集計結果です。".to_string(),
            color,
            fields: vec![
                EmbedField {
                    name: "総トレード数".to_string(),
                    value: format!("{} 回", total_trades),
                    inline: true,
                },
                EmbedField {
                    name: "勝率 (勝/敗)".to_string(),
                    value: format!("{:.1} % ({}勝 / {}敗)", win_rate, win_trades, total_trades - win_trades),
                    inline: true,
                },
                EmbedField {
                    name: "累計仮想損益".to_string(),
                    value: format!("**{}{:.0} 円**", if total_pl >= 0.0 { "+" } else { "" }, total_pl),
                    inline: false,
                },
            ],
        }],
    };

    let client = reqwest::Client::new();
    client.post(&webhook_url).json(&payload).send().await?;
    Ok(())
}
