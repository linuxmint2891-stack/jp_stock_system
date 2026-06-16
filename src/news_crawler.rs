use tokio::time::{sleep, Duration};
use crate::news_merger::NewsData;
use regex::Regex;

/// 指定された銘柄コード（複数）の最新ニュースをYahoo!ファイナンスの個別ニュースページからスクレイピング取得する
pub async fn fetch_real_news_for_codes(codes: &[String]) -> Vec<NewsData> {
    let mut results = Vec::new();
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
        .unwrap();

    // ニュースタイトルを抽出するための正規表現
    // Yahoo!ファイナンスのHTML構造に基づき、NewsItem__heading クラスを持つ h3 タグを抽出
    let re = Regex::new(r#"<h3[^>]*class="[^"]*NewsItem__heading[^"]*"[^>]*>(.*?)</h3>"#).unwrap();

    println!("📡 ネット上から {} 件の銘柄の最新ニュースを収集します (スクレイピング方式)...", codes.len());

    for raw_code in codes {
        // 末尾の「0」（パリティや桁数調整用）をカットして純粋な4桁（または5桁）の銘柄コードにする
        let clean_code = if raw_code.ends_with('0') && raw_code.len() > 4 {
            &raw_code[0..raw_code.len() - 1]
        } else {
            raw_code.as_str()
        };

        // Yahoo!ファイナンスの個別銘柄ニュースURL
        let url = format!("https://finance.yahoo.co.jp/quote/{}.T/news", clean_code);
        
        println!("🌐 取得中: {} (コード: {})", url, clean_code);

        match client.get(&url).send().await {
            Ok(response) => {
                if let Ok(html) = response.text().await {
                    let mut combined_news = String::new();
                    let mut count = 0;

                    // 正規表現でタイトルを抽出
                    for cap in re.captures_iter(&html) {
                        let title = &cap[1];
                        // HTMLエンティティやタグの簡易除去（必要に応じて強化）
                        let clean_title = title.replace("&amp;", "&")
                            .replace("&quot;", "\"")
                            .replace("<span class=\"_Highlight_8bjpa_1\">", "")
                            .replace("</span>", "");
                        
                        combined_news.push_str(&format!("【ニュース{}】{} ", count + 1, clean_title));
                        count += 1;
                        if count >= 5 { break; } // 直近5件まで
                    }

                    // ニュースが取得できていれば格納
                    if !combined_news.trim().is_empty() {
                        let mut final_code = raw_code.clone();
                        // 5桁形式（末尾0）に統一してマージの失敗を防ぐ
                        if final_code.len() == 4 {
                            final_code.push('0');
                        }

                        results.push(NewsData {
                            code: final_code, 
                            news_text: combined_news,
                        });
                    } else {
                        println!("⚠️  コード {} のニュースは見つかりませんでした。", clean_code);
                    }
                }
            }
            Err(e) => {
                eprintln!("⚠️  コード {} のニュース取得に失敗しました: {}", clean_code, e);
            }
        }

        // Yahoo側のサーバーに負荷をかけないよう、1リクエストごとに1.0秒のウェイトを入れる（マナー・規制回避）
        sleep(Duration::from_millis(1000)).await;
    }

    println!("✅ ニュースの収集完了。取得できた銘柄数: {}/{}", results.len(), codes.len());
    results
}
