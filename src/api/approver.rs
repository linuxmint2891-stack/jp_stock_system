use super::ai_engine::{analyze_news_with_gemma, apply_guardrail, SentimentResult};
use anyhow::Result;

/// 注文承認を判断する構造体
pub struct TradeApprover;

impl TradeApprover {
    /// 銘柄情報とニュースを元に、AI（Gemini）に注文の承認（GO/NO-GO）を仰ぐ
    pub async fn approve_trade(
        code: &str,
        name: &str,
        price: f64,
        change_pct: f64,
        news_text: &str,
    ) -> Result<SentimentResult> {
        println!("🤖 AI（Gemini）に注文承認をリクエスト中... [{} {}]", code, name);

        // プロンプトの構築（ノイズキャンセリング・鉄の掟を強化）
        let prompt = format!(
            r#"あなたはプロのクオンツ・システムトレーダーです。
提供された定量データと定性ニュースを客観的に分析し、指定されたJSONフォーマットでのみ回答してください。

# 分析対象データ
- 銘柄コード: {}
- 銘柄名: {}
- 株価: {}円
- 前日比: {}%
- 直近のニュース/開示情報: "{}"

# ★投資判断の厳格な掟（ノイズキャンセリング）
1. 【個体識別】分析対象は、指定された【銘柄名】および【銘柄コード】のみです。
2. 【ノイズ除去】入力されたニュース本文に「他社の社名」や「市場全体のまとめ情報」が含まれている場合、それらは完全に無視してください。
3. 【固有材料の重視】分析対象の銘柄そのものに関する固有の好材料（決算、プレスリリース、提携など）が直接確認できない場合は、原則として sentiment_score を 0.45 以下とし、has_distinct_material を false としてください。
4. 【決算情報の特例】ニュースに「上方修正」「最高益」「黒字浮上」「増配」などの具体的な決算・業績に関するポジティブなキーワードが含まれている場合、それは強力な【固有材料】とみなします。この場合、has_distinct_material を true とし、自信を持って 0.70 以上のスコアを検討してください。
5. 【GO判定の条件】「強力な固有材料」と「テクニカル面（前日比プラスなど）」の両方が揃った場合のみ、decision を "GO"、sentiment_score を 0.70 〜 1.00 と判定してください。
6. 【スコアの傾斜】材料が乏しく見送り(NO-GO)とする場合でも、テクニカルが非常に強力であったり、期待感のあるニュースが含まれる場合は、将来の監視対象として 0.40 〜 0.45 のスコアを付与し、その理由を明記してください。

# 🛠️ 厳格な禁止事項とスコアペナルティ（最優先）
1. 「株価の上昇」「ストップ高」などの【結果】のみを「材料（原因）」とみなしてはならない。これらしかない場合は、一律 0.30 以下のスコアとし、has_distinct_material を false とせよ。
2. ニュース本文が他社の情報のみで、対象銘柄自体のプレスリリースやIR情報が一切ない場合、どれほど株価が急騰していても一律 0.30 以下のスコアとし、has_distinct_material を false とせよ。
3. あなた自身が「リスク（risk_factor）」の項目に「固有の材料が確認できない」と記述する場合、スコアは必ず 0.45 以下、判定は必ず NO-GO とせよ。

# 出力フォーマット (JSONのみ)
{{
  "has_distinct_material": true または false,
  "sentiment_score": 浮動小数点数（-1.0〜1.0）,
  "reasons": ["理由1", "理由2"],
  "risk_factor": "最も注意すべき懸念点",
  "decision": "GO" または "NO-GO"
}}"#,
            code, name, price, change_pct, news_text
        );

        let result = analyze_news_with_gemma(&prompt).await
            .map_err(|e| anyhow::anyhow!("AI分析エラー: {}", e))?;

        // ガードレールを適用
        let guarded_result = apply_guardrail(result);

        Ok(guarded_result)
    }
}
