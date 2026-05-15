use polars::prelude::*;
use std::fs::File;
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct NewsData {
    pub code: String,
    pub news_text: String,
}

pub fn merge_news_to_parquet(path: &str, news: Vec<NewsData>) -> Result<()> {
    println!("📖 Loading parquet from {}...", path);
    // 1. 既存の Parquet を読み込む
    let mut df = LazyFrame::scan_parquet(path, Default::default())?.collect()?;

    println!("📊 Creating news dataframe...");
    // 2. ニュースデータを DataFrame に変換
    let codes: Vec<String> = news.iter().map(|n| n.code.clone()).collect();
    let texts: Vec<String> = news.iter().map(|n| n.news_text.clone()).collect();

    let news_df = df!(
        "Code" => codes,
        "news_text" => texts,
    )?;

    println!("🔗 Joining news data...");
    // 3. 結合
    // もし既に news_text があるなら削除（上書きするため）
    if df.get_column_names().contains(&"news_text") {
        df = df.drop("news_text")?;
    }

    let mut joined_df = df.lazy()
        .left_join(news_df.lazy(), col("Code"), col("Code"))
        .collect()?;

    println!("💾 Saving updated parquet to {}...", path);
    // 4. 保存
    let file = File::create(path)?;
    ParquetWriter::new(file).finish(&mut joined_df)?;

    println!("✅ Merge completed successfully.");
    Ok(())
}
