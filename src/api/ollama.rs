use serde::{Deserialize, Serialize};
use reqwest::Client;
use serde_json::json;

#[derive(Serialize, Deserialize, Debug)]
pub struct SentimentResult {
    pub sentiment_score: f64,      // -1.0（極めてネガティブ）〜 1.0（極めてポジティブ）
    pub reasons: Vec<String>,      // そう判断した理由（複数）
    pub risk_factor: String,       // 懸念されるリスク要素
    pub decision: String,          // "GO" または "NO-GO"
}

/// プロンプトをGemma 3に送信し、JSON形式で結果を取得する
pub async fn analyze_news_with_gemma(prompt: &str) -> Result<SentimentResult, Box<dyn std::error::Error>> {
    let client = Client::new();
    let url = "http://localhost:11434/api/generate";

    // Ollama API へのリクエストペイロード
    let payload = json!({
        "model": "gemma3:latest",
        "prompt": prompt,
        "stream": false,
        "format": "json", // OllamaにJSON出力を強制させるモード
        "options": {
            "temperature": 0.0
        }
    });

    // 送信
    let response = client.post(url)
        .json(&payload)
        .send()
        .await?;

    let response_json: serde_json::Value = response.json().await?;
    
    // 生成されたレスポンスのテキスト部分を抽出
    let raw_response = response_json["response"]
        .as_str()
        .ok_or("Failed to get response text from Ollama")?;

    // テキストをパースして構造体に変換
    let result: SentimentResult = serde_json::from_str(raw_response)?;

    Ok(result)
}
