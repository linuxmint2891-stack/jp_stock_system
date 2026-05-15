use super::ollama::{analyze_news_with_gemma, SentimentResult};
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
            "あなたはプロのクオンツ・システムトレーダーです。\n\
             提供された定量データと定性ニュースを客観的に分析し、指定されたJSONフォーマットでのみ回答してください。\n\n\
             # 分析対象データ\n\
             - 銘柄コード: {}\n\
             - 銘柄名: {}\n\
             - 株価: {}円\n\
             - 前日比: {}%\n\
             - 直近のニュース/開示情報: \"{}\"\n\n\
             # ★投資判断の厳格な掟（ノイズキャンセリング）\n\
             1. 【個体識別】分析対象は、指定された【銘柄名】および【銘柄コード】のみです。\n\
             2. 【ノイズ除去】入力されたニュース本文に「他社の社名」や「市場全体のまとめ情報（例：ゴールデンクロス銘柄一覧、〇〇セクターの動向）」が含まれている場合、それらはすべて「ノイズ」として完全に無視してください。\n\
             3. 【他社情報の排除】他社の好決算・悪決算や、市場全体のテクニカル動向を、分析対象銘柄の判断材料にしては絶対になりません。\n\
             4. 【固有材料の重視】対象銘柄「そのもの」に関する固有の好材料（例：対象銘柄自体の決算、プレスリリース、固有の提携ニュースなど）が直接確認できない場合は、テクニカルが良くても「材料なし」とみなし、無条件で decision を \"NO-GO\"、sentiment_score を 0.0 としてください。\n\
             5. 【GO判定の条件】対象銘柄「そのもの」に強力な固有の好材料（上方修正、好決算、画期的な新製品発表など）が確認され、テクニカル面（前日比プラスなど）も一致している場合のみ、自信を持って decision を \"GO\"、sentiment_score を 0.70 〜 1.00 と判定してください。\n\n\
             # 出力フォーマット (JSONのみ)\n\
             {{\n\
               \"sentiment_score\": 浮動小数点数（-1.0〜1.0）,\n\
               \"reasons\": [\"理由1\", \"理由2\"],\n\
               \"risk_factor\": \"最も注意すべき懸念点\",\n\
               \"decision\": \"GO\" または \"NO-GO\"\n\
             }}",
            code, name, price, change_pct, news_text
        );

        let result = analyze_news_with_gemma(&prompt).await
            .map_err(|e| anyhow::anyhow!("Ollama分析エラー: {}", e))?;

        Ok(result)
    }
}
