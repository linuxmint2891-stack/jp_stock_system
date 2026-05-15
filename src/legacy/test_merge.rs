use jp_stock_system::news_merger::{merge_news_to_parquet, NewsData};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 ニュースマージのテストを開始します...");

    // テスト用のニュースデータ
    // 今回実行ログに出ていた「ラクス(39230)」と「サンウェルズ(92290)」に対して本物のニュースを模擬
    let test_news = vec![
        NewsData {
            code: "39230".to_string(),
            news_text: "ラクスが発表した月次国内売上高は前年同月比38%増と絶好調。楽楽精算の導入企業数が大企業を中心に急増しており、今期の業績上振れ観測が強まる。".to_string(),
        },
        NewsData {
            code: "92290".to_string(),
            news_text: "サンウェルズは、厚生労働省の定める施設基準を満たした新規の介護医療院を今期中に3箇所追加開設すると公表。初期コストが先行するものの、中期的な収益基盤の強化に繋がる。".to_string(),
        },
        // Sansan(44430)、robot home(14350)、INTLOOP(95560) にはあえてニュースを登録しない（空欄の挙動を見るため）
    ];

    // マージの実行
    merge_news_to_parquet("data/processed_market_data.parquet", test_news)?;

    println!("✅ テストデータのマージが完了しました。この状態で cargo run して TUI の AIスカウトを実行してみてください！");
    Ok(())
}
