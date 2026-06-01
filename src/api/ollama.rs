use serde::{Deserialize, Serialize};
use reqwest::Client;
use serde_json::json;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SentimentResult {
    pub has_distinct_material: bool, // 固有の材料が直接確認できるか
    pub sentiment_score: f64,      // -1.0（極めてネガティブ）〜 1.0（極めてポジティブ）
    pub reasons: Vec<String>,      // そう判断した理由（複数）
    pub risk_factor: String,       // 懸念されるリスク要素
    pub decision: String,          // "GO" または "NO-GO"
}

/// AIの出力を検証し、強制NO-GOのガードレールを適用する
pub fn apply_guardrail(mut res: SentimentResult) -> SentimentResult {
    // 固有材料が「ない（false）」と判定されている場合
    if !res.has_distinct_material {
        // AIのスコアがどうあれ、強制的にNO-GOライン（0.30以下）に固定
        if res.sentiment_score > 0.30 {
            res.sentiment_score = 0.30;
        }
        res.decision = "NO-GO".to_string();
        
        // 理由の先頭にガードレール発動の旨を追記
        let warning = "[Guardrail Triggered: No Distinct Material]".to_string();
        if !res.reasons.contains(&warning) {
            res.reasons.insert(0, warning);
        }
    } else {
        // 固有材料があり、かつスコアがGO基準（0.70以上）を満たしているか確認
        if res.sentiment_score >= 0.70 {
            res.decision = "GO".to_string();
        } else {
            res.decision = "NO-GO".to_string();
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guardrail_triggered() {
        // AIが「材料なし」としながらも、なぜか高いスコア（0.75）を出してしまったケース
        let mock_ai_response = SentimentResult {
            has_distinct_material: false,
            sentiment_score: 0.75,
            reasons: vec!["連日ストップ高で急上昇しており勢いがあるため。".to_string()],
            risk_factor: "固有材料なし".to_string(),
            decision: "GO".to_string(),
        };

        let result = apply_guardrail(mock_ai_response);

        // ガードレールにより、強制的にGO判定が却下され、スコアが落とされていること
        assert_eq!(result.decision, "NO-GO");
        assert!(result.sentiment_score <= 0.30);
        assert!(result.reasons[0].contains("[Guardrail Triggered"));
    }

    #[test]
    fn test_guardrail_pass() {
        // 正当な材料があり、スコアも高いケース
        let mock_ai_response = SentimentResult {
            has_distinct_material: true,
            sentiment_score: 0.85,
            reasons: vec!["〇〇材料の適時開示を確認。".to_string()],
            risk_factor: "特になし".to_string(),
            decision: "GO".to_string(),
        };

        let result = apply_guardrail(mock_ai_response);

        assert_eq!(result.decision, "GO");
        assert_eq!(result.sentiment_score, 0.85);
    }
}

/// プロンプトをGemma 3に送信し、JSON形式で結果を取得する
pub async fn analyze_news_with_gemma(prompt: &str) -> Result<SentimentResult, Box<dyn std::error::Error>> {
    let client = Client::new();
    
    // 環境変数 OLLAMA_URL があればそれを使う。なければデフォルト
    let base_url = std::env::var("OLLAMA_URL").unwrap_or_else(|_| "http://localhost:11434".to_string());
    let url = format!("{}/api/generate", base_url);

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
