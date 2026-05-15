use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Serialize, Deserialize, Debug)]
struct SentimentResult {
    sentiment_score: f64,
    reasons: Vec<String>,
    risk_factor: String,
    decision: String,
}

async fn analyze_news_with_gemma(news_text: &str) -> Result<SentimentResult, Box<dyn std::error::Error>> {
    let client = Client::new();
    let url = "http://localhost:11434/api/generate";

    let prompt = format!(
        "あなたはプロのクオンツ・システムトレーダーです。\n\
         以下のニュースまたは適時開示情報を客観的に分析し、指定されたJSONフォーマットでのみ回答してください。\n\n\
         # ニュース本文\n\
         {}\n\n\
         # 制約事項\n\
         - 必ず有効なJSONフォーマットのみを出力してください。\n\
         - 余計な解説や、マークダウン（```json などの囲み）は一切含めないでください。\n\n\
         - ニュース本文（定性情報）が「空」または「詳細不明」な場合は、テクニカル指標（騰落率や出来高など）がどれだけ良くても、決して投資判断を「GO」にしてはなりません。その場合は「NO-GO（スコア: 0.0）」とし、理由に「材料不明のため見送り」と明記してください。\n\
         - テクニカル指標（定量）とニュース材料（定性）の「両方」に強い裏付けがある場合のみ、スコアを「0.7以上」のGOとして承認してください。\n\
         # 出力フォーマット\n\
         {{\n\
           \"sentiment_score\": -1.0から1.0の範囲の浮動小数点数,\n\
           \"reasons\": [\"理由1\", \"理由2\"],\n\
           \"risk_factor\": \"最も注意すべき懸念点（なければ「特になし」）\",\n\
           \"decision\": \"GO\" または \"NO-GO\"\n\
         }}",
        news_text
    );

    let payload = json!({
        "model": "gemma3", // 先ほどcurlで成功したモデル名「gemma3」に指定
        "prompt": prompt,
        "stream": false,
        "format": "json",
        "options": {
            "temperature": 0.0
        }
    });

    let response = client.post(url)
        .json(&payload)
        .send()
        .await?;

    let response_json: serde_json::Value = response.json().await?;
    let raw_response = response_json["response"]
        .as_str()
        .ok_or("Failed to get response text from Ollama")?;

    let result: SentimentResult = serde_json::from_str(raw_response)?;
    Ok(result)
}

#[tokio::main]
async fn main() {
    let sample_news = "本日、ABC株式会社は今期の純利益予想を上方修正し、前年同期比で35%増となる見通しを発表した。同時に、年間配当を1株あたり10円増配すること、および上限50億円の自社株買いの実施も公表。市場では好感した買いが集まっている。";

    println!("Gemma 3 による分析を開始します...");
    
    match analyze_news_with_gemma(sample_news).await {
        Ok(analysis) => {
            println!("\n=== 分析完了 ===");
            println!("意思決定: {}", analysis.decision);
            println!("スコア  : {}", analysis.sentiment_score);
            println!("判断理由:");
            for reason in analysis.reasons {
                println!(" - {}", reason);
            }
            println!("懸念リスク: {}", analysis.risk_factor);
            
            if analysis.decision == "GO" && analysis.sentiment_score >= 0.5 {
                println!("\n➔ [注文執行] クオンツ条件・AI感情分析ともにクリア。買い注文を発注します。");
            } else {
                println!("\n➔ [見送り] AI判定がNO-GO、またはスコアが基準値未満です。");
            }
        }
        Err(e) => {
            eprintln!("エラーが発生しました: {}", e);
        }
    }
}