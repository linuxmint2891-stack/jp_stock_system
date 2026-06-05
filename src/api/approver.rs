use super::ollama::{analyze_news_with_gemma, apply_guardrail, SentimentResult};
use anyhow::Result;

/// 注文承認を判断する構造体
pub struct TradeApprover;

impl TradeApprover {
    /// 銘柄情報とニュースを元に、Ollamaに注文の承認（GO/NO-GO）を仰ぐ
    pub async fn approve_trade(
        code: &str,
        name: &str,
        price: f64,
        change_pct: f64,
        news_text: &str,
    ) -> Result<SentimentResult> {
        println!("🤖 Ollamaに注文承認をリクエスト中... [{} {}]", code, name);

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
2. 【ノイズ除去】入力されたニュース本文に「他社の社名」や「市場全体のまとめ情報（例：ゴールデンクロス銘柄一覧、〇〇セクターの動向）」が含まれている場合、それらはすべて「ノイズ」として完全に無視してください。
3. 【他社情報の排除】他社の好決算・悪決算や、市場全体のテクニカル動向を、分析対象銘柄の判断材料にしては絶対になりません。
4. 【固有材料の重視】対象銘柄「そのもの」に関する固有の好材料（例：対象銘柄自体の決算、プレスリリース、固有の提携ニュースなど）が直接確認できない場合は、テクニカルが良くても「材料なし」とみなし、無条件で decision を "NO-GO"、sentiment_score を 0.0、has_distinct_material を false としてください。
5. 【GO判定の条件】対象銘柄「そのもの」に強力な固有の好材料（上方修正、好決算、画期的な新製品発表など）が確認され、テクニカル面（前日比プラスなど）も一致している場合のみ、自信を持って decision を "GO"、sentiment_score を 0.70 〜 1.00、has_distinct_material を true と判定してください。
6. 【矛盾の排除】理由(reasons)とリスク(risk_factor)の間で論理的な矛盾を絶対に起こさないでください。例えば、理由で「好材料がある」と書きながら、リスクで「固有の材料が確認できない」と述べるのは厳禁です。材料がない場合は、理由にも「固有の材料なし」と明記してください。

# 🛠️ 厳格な禁止事項とスコアペナルティ（最優先）
1. 「株価の上昇」「ストップ高（S高）」「連日続伸」「年初来高値更新」などの【価格の動き（結果）】そのものは、いかなる場合も「企業固有の材料（原因）」とみなしてはならない。これらしか記載がない場合は、一律NO-GO（スコア0.30以下、可能な限り0.00）とし、has_distinct_material を false とせよ。
2. ニュース本文に「他社の情報」や「市場全体の動向（日経平均、セクター上昇など）」しかなく、対象銘柄自体のプレスリリースやIR情報が一切ない場合、どれほど株価が急騰していても一律NO-GO（スコア0.30以下）とし、has_distinct_material を false とせよ。
3. あなた自身が「リスク（risk_factor）」の項目に「固有の材料が確認できない」と記述する場合は、論理的整合性を保つため、スコアは必ず0.30以下、判定は必ずNO-GO、has_distinct_material は false としなければならない。0.40以上のスコアをつけることは自己矛盾であり、厳禁とする。

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
            .map_err(|e| anyhow::anyhow!("Ollama分析エラー: {}", e))?;

        // ガードレールを適用
        let guarded_result = apply_guardrail(result);

        Ok(guarded_result)
    }
}
