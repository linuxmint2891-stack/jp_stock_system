use jp_stock_system::api::approver::TradeApprover;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🚀 [AI承認デモ] Ollamaによる注文承認プロセスを開始します...");

    // テスト用の銘柄データ
    let test_trades = vec![
        ("7203", "トヨタ自動車", 3500.0, "円安進行による輸出採算改善の期待。PBR1倍割れで割安感あり。"),
        ("9984", "ソフトバンクグループ", 8500.0, "傘下ARMの株価急騰による資産価値増大。AI関連銘柄としての買い。"),
        ("4661", "オリエンタルランド", 4800.0, "国内レジャー需要は堅調だが、PERが高水準で調整局面の可能性。"),
    ];

    for (code, name, price, reason) in test_trades {
        println!("\n--------------------------------------------------");
        println!("【注文検討】 {} {} (株価: {}円)", code, name, price);
        println!("【テクニカル/材料】 {}", reason);

        match TradeApprover::approve_trade(code, name, price, 0.0, reason).await {
            Ok(result) => {
                println!("--- Ollamaの判断 ---");
                println!("判定: {}", result.decision);
                println!("感情スコア: {}", result.sentiment_score);
                println!("判断理由:");
                for r in result.reasons {
                    println!(" - {}", r);
                }
                println!("リスク要因: {}", result.risk_factor);

                if result.decision == "GO" {
                    println!("\n✅ [注文執行] AIの承認が得られました。発注します。");
                } else {
                    println!("\n❌ [注文見送り] AIの承認が得られませんでした。");
                }
            }
            Err(e) => {
                eprintln!("❌ エラー: {}", e);
            }
        }
    }

    println!("\n--------------------------------------------------");
    println!("🏁 デモ終了");
    Ok(())
}
