use reqwest::Client;
use serde_json::json;

/// Discord Bot API経由でメッセージを送信する
pub async fn send_discord_bot_notification(
    token: &str,
    channel_id: &str,
    code: &str,
    name: &str,
    price: f64,
    score: f64,
    reason: &str,
    risk: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let url = format!("https://discord.com/api/v10/channels/{}/messages", channel_id);

    let embed = json!({
        "title": format!("🟢 【AI承認】新規買いシグナル発生: {} ({})", name, code),
        "color": 3066993,
        "fields": [
            {
                "name": "💰 検出価格 / 判定スコア",
                "value": format!("価格: **{}円** \nスコア: **{:.2}**", price, score),
                "inline": true
            },
            {
                "name": "📝 承認理由",
                "value": reason,
                "inline": false
            },
            {
                "name": "⚠️ 検出されたリスク要因",
                "value": risk,
                "inline": false
            }
        ],
        "footer": {
            "text": "※本情報はシステムによる自動分析であり、投資判断は自己責任で行ってください。"
        }
    });

    let payload = json!({
        "embeds": [embed]
    });

    let response = client.post(&url)
        .header("Authorization", format!("Bot {}", token))
        .json(&payload)
        .send()
        .await?;

    if response.status().is_success() {
        println!("📢 Discord Bot経由でAIスカウト通知を送信しました！");
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        eprintln!("❌ Discord Bot通知の送信に失敗しました: {} - {}", status, body);
    }

    Ok(())
}

/// Discord Webhook経由でAIスカウトの承認結果を通知する
pub async fn send_discord_notification(
    webhook_url: &str,
    code: &str,
    name: &str,
    price: f64,
    score: f64,
    reason: &str,
    risk: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();

    let payload = json!({
        "username": "AIトレーディングスカウト",
        "embeds": [{
            "title": format!("🟢 【AI承認】新規買いシグナル発生: {} ({})", name, code),
            "color": 3066993,
            "fields": [
                {
                    "name": "💰 検出価格 / 判定スコア",
                    "value": format!("価格: **{}円** \nスコア: **{:.2}**", price, score),
                    "inline": true
                },
                {
                    "name": "📝 承認理由",
                    "value": reason,
                    "inline": false
                },
                {
                    "name": "⚠️ 検出されたリスク要因",
                    "value": risk,
                    "inline": false
                }
            ],
            "footer": {
                "text": "※本情報はシステムによる自動分析であり、投資判断は自己責任で行ってください。"
            }
        }]
    });

    let response = client.post(webhook_url).json(&payload).send().await?;

    if response.status().is_success() {
        println!("📢 Discord Webhook経由でAIスカウト通知を送信しました！");
    } else {
        eprintln!("❌ Discord Webhook通知の送信に失敗しました: {}", response.status());
    }

    Ok(())
}
