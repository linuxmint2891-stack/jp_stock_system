use std::process::Command;
use std::fs;
use chrono::{Local, Timelike, Utc, FixedOffset};
use anyhow::Result;
use jp_stock_system::utils::settings::Settings;

#[tokio::main]
async fn main() -> Result<()> {
    let trading = Settings::new()?.trading;
    // 🚨 朝 8:55 を過ぎてジョブが動いた場合は、危険なので強制停止する（FORCE_RUN環境変数が設定されている場合はスキップ）
    let jst_offset = FixedOffset::east_opt(9 * 3600).unwrap();
    let now = Utc::now().with_timezone(&jst_offset);
    let force_run = std::env::var("FORCE_RUN").is_ok();
    if !force_run && (now.hour() >= 9 || (now.hour() == 8 && now.minute() >= 55)) {
        println!("🛑 [🚨緊急停止ガードレール発動] 現在時刻は JST {} です。", now.format("%H:%M:%S"));
        println!("朝 8:55 までの事前予約に間に合わなかったため、本日の自動売買は安全のためにスキップします。");
        println!("（テスト等で強制実行したい場合は、環境変数 FORCE_RUN=true を指定してください）");
        std::process::exit(0);
    }
    println!(
        "🟢 時間内（JST {}）の起動を確認。{}向けの手動注文提案を生成します...",
        now.format("%H:%M:%S"),
        trading.broker
    );

    let today = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    println!("==================================================");
    println!("🚀 全自動AIトレーディングシステム 始動: {}", today);
    println!("==================================================");

    // --- フォルダの事前準備 ---
    fs::create_dir_all("data")?;
    fs::create_dir_all("logs")?;

    // ==================================================
    // 1. 【Phase 1: 収集】最新の市場データ(東証全銘柄)の同期
    // ==================================================
    println!("\n📥 [Phase 1] 市場データの同期を開始 (sync_yahoo)...");
    
    let status_sync = Command::new("cargo")
        .args(["run", "--release", "--bin", "sync_yahoo"])
        .status()?;

    if !status_sync.success() {
        return Err(anyhow::anyhow!("❌ Phase 1: 市場データの同期に失敗しました。"));
    }
    println!("✅ Phase 1 完了: 市場データが最新に更新されました。");

    // ==================================================
    // 2. 【Phase 2, 3 & 4】AI分析・執行ライフサイクル (AiScout)
    // ==================================================
    // ai_scout.rs が以下の処理を統合して実行します：
    // - Phase 2: トレンド検知および対象銘柄の最新ニュース取得・マージ
    // - Phase 3: Ollama (Gemma 3) による定性投資判断
    // - Phase 4: Discord通知およびペーパートレード台帳への記録
    
    println!("\n🧠 [Phase 2, 3 & 4] AIスカウトおよび執行パイプラインを開始...");
    
    let status_aiscout = Command::new("cargo")
        .args(["run", "--release", "--bin", "ai_scout"])
        .status()?;

    if !status_aiscout.success() {
        return Err(anyhow::anyhow!("❌ AIスカウトの実行中にエラーが発生しました。"));
    }
    
    println!("\n✅ AIスカウトパイプラインが正常に完了しました。");

    // ==================================================
    // 5. 【Future Phase】証券会社APIへの自動発注
    // ==================================================
    // moomoo証券の日本株・単元未満株は手動発注運用のため、
    // paper_trade の売買提案を実注文として送信しない。
    
    println!("\n==================================================");
    println!("🏁 すべてのパイプライン処理が正常に終了しました。");
    println!("==================================================");
    Ok(())
}
