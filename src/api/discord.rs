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

/// 🛒 注文の約定（買い・売り）をDiscordに通知する
pub async fn notify_order_execution(
    code: &str,
    is_buy: bool,
    price: f64,
    qty: i64,
    pl_amount: Option<f64>,
    pl_pct: Option<f64>,
) -> Result<(), reqwest::Error> {
    let webhook_url = match env::var("DISCORD_WEBHOOK_URL") {
        Ok(url) => url,
        Err(_) => return Ok(()),
    };

    let title = if is_buy {
        format!("🛒 【買い約定】 銘柄: {}", code)
    } else {
        format!("💰 【売り約定】 銘柄: {}", code)
    };

    let color = if is_buy {
        3066993 // 緑色 (Hex: #2ecc71)
    } else {
        let pl = pl_amount.unwrap_or(0.0);
        if pl >= 0.0 {
            3447003 // 青色系 (Hex: #3498db)
        } else {
            15158332 // 赤色系 (Hex: #e74c3c)
        }
    };

    let mut fields = vec![
        EmbedField {
            name: "約定価格".to_string(),
            value: format!("{:.1} 円", price),
            inline: true,
        },
        EmbedField {
            name: "数量".to_string(),
            value: format!("{} 株", qty),
            inline: true,
        },
    ];

    if let (Some(pl), Some(pct)) = (pl_amount, pl_pct) {
        fields.push(EmbedField {
            name: "実現損益".to_string(),
            value: format!("{:.0} 円 ({:.2}%)", pl, pct),
            inline: false,
        });
    }

    let payload = DiscordWebhookPayload {
        username: "株AIスカウトシステム".to_string(),
        avatar_url: None,
        embeds: vec![DiscordEmbed {
            title,
            description: "予約注文が当日の始値で約定されました。".to_string(),
            color,
            fields,
        }],
    };

    let client = reqwest::Client::new();
    client.post(&webhook_url).json(&payload).send().await?;
    Ok(())
}

/// 📊 【AIペーパートレード 運用パフォーマンス報告】 をDiscordへ送信する
pub async fn notify_portfolio_summary_report(
    date: &str,
    positions: &[(String, String, f64, f64, i64, f64, f64)], // (code, name, entry_price, current_price, qty, pl_amount, pl_pct)
    total_unrealized: f64,
    total_realized: f64,
    total_trades: i64,
    win_trades: i64,
    win_rate: f64,
    profit_factor: f64,
) -> Result<(), reqwest::Error> {
    let webhook_url = match env::var("DISCORD_WEBHOOK_URL") {
        Ok(url) => url,
        Err(_) => return Ok(()),
    };

    let color = if total_realized + total_unrealized >= 0.0 {
        15844367 // 金色 (Hex: #f1c40f)
    } else {
        9807270  // グレー (Hex: #95a5a6)
    };

    let mut description = format!(
        "📅 **集計日**: {}\n\n**現時点での保有ポジション (含み損益):**\n",
        date
    );

    if positions.is_empty() {
        description.push_str("• なし\n");
    } else {
        for (code, name, entry_price, current_price, qty, pl_amount, pl_pct) in positions {
            description.push_str(&format!(
                "• **{} {}** ({}株)\n  購入: {:.1}円 -> 現在: {:.1}円 ({:+.2}%) | 評価損益: {:+.0}円\n",
                code, name, qty, entry_price, current_price, pl_pct, pl_amount
            ));
        }
    }

    description.push_str("\n-----------------------------------------\n");
    description.push_str(&format!(
        "💰 **資産状況サマリー:**\n• 総含み損益 (評価損益) : **{:+.0} 円**\n• 通算確定損益 (実現損益) : **{:+.0} 円**\n\n",
        total_unrealized, total_realized
    ));

    description.push_str(&format!(
        "📈 **AIスコア運用の通算成績:**\n• 総トレード回数 : {} 回\n• 勝敗 : {}勝 {}敗 (勝率: {:.1}%)\n• プロフィットファクター : {:.2}",
        total_trades,
        win_trades,
        total_trades - win_trades,
        win_rate,
        profit_factor
    ));

    let payload = DiscordWebhookPayload {
        username: "株AIスカウトシステム".to_string(),
        avatar_url: None,
        embeds: vec![DiscordEmbed {
            title: "📊 AIペーパートレード 運用パフォーマンス報告".to_string(),
            description,
            color,
            fields: vec![],
        }],
    };

    let client = reqwest::Client::new();
    client.post(&webhook_url).json(&payload).send().await?;
    Ok(())
}

/// 📊 ポートフォリオのサマリー（テキスト）をファイルとしてDiscordに添付送信する
pub async fn send_portfolio_file_to_discord(
    webhook_url: &str,
    file_path: &str,
    message_content: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    use reqwest::multipart;

    // 1. 送信するファイルをバイト列として読み込む
    let file_bytes = std::fs::read(file_path)?;
    let file_part = multipart::Part::bytes(file_bytes)
        .file_name("portfolio_report.txt")
        .mime_str("text/plain")?;

    // 2. マルチパートフォームを作成
    let form = multipart::Form::new()
        .text("content", message_content.to_string())
        .part("file", file_part);

    // 3. WebhookにPOSTリクエストを送信
    let response = client.post(webhook_url).multipart(form).send().await?;

    if response.status().is_success() {
        println!("🚀 Discordにポートフォリオのファイルログを送信しました。");
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        eprintln!("❌ Discordへのファイル送信に失敗しました: {} - {}", status, body);
    }

    Ok(())
}

