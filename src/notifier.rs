use reqwest::Client;
use serde_json::json;

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

    // 視覚的に分かりやすく整理されたリッチな通知（埋め込みメッセージ）を構成
    let payload = json!({
        "username": "AIトレーディングスカウト",
        "avatar_url": "https://i.imgur.com/4E7N8XW.png", // ロボットの適当なアイコンURL
        "embeds": [{
            "title": format!("🟢 【AI承認】新規買いシグナル発生: {} ({})", name, code),
            "color": 3066993, // グリーン系のカラーコード
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

    let url = webhook_url.trim();
    println!("🔍 Sending to Discord: {} (len: {})", url, url.len());

    let response = client.post(url).json(&payload).send().await?;

    if response.status().is_success() {
        println!("📢 DiscordへAIスカウト通知を送信しました！");
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        eprintln!("❌ Discord通知の送信に失敗しました: {} - {}", status, body);
    }

    Ok(())
}
