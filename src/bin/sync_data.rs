use polars::prelude::*;
use polars_ops::pivot::{pivot, PivotAgg};
use jp_stock_system::utils::io;

fn main() -> PolarsResult<()> {
    println!("Starting data synchronization from a single large CSV...");

    // 1. 設定：J-Quants V2から取得した最新のJSONデータ（拡張子は .csv のまま保存されていますが中身は JSON です）
    let input_file = "data/all_stocks_daily.json";
    let output_parquet = "data/processed_market_data.parquet";

    // 2. JSONの読み込み
    println!("Reading JSON from {}...", input_file);
    let file = std::fs::File::open(input_file).map_err(|e| PolarsError::ComputeError(format!("IO Error: {}", e).into()))?;
    
    // {"daily_quotes": [...]} または {"data": [ ... ]} の形式を展開して取得
    let json_value: serde_json::Value = serde_json::from_reader(file).map_err(|e| PolarsError::ComputeError(format!("JSON Error: {}", e).into()))?;
    let daily_quotes = json_value.get("daily_quotes")
        .or_else(|| json_value.get("data"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| PolarsError::ComputeError("Missing daily_quotes or data array".into()))?;

    // JSON配列から DataFrame に直接変換するための一時バッファ（メモリ上で処理）
    let mut dates = Vec::with_capacity(daily_quotes.len());
    let mut codes = Vec::with_capacity(daily_quotes.len());
    let mut adj_closes = Vec::with_capacity(daily_quotes.len());

    for item in daily_quotes {
        if let (Some(d), Some(c), Some(ac)) = (
            item.get("Date").and_then(|v| v.as_str()),
            item.get("Code").and_then(|v| v.as_str()),
            item.get("AdjustmentClose").or_else(|| item.get("AdjC")).and_then(|v| v.as_f64()),
        ) {
            dates.push(d.to_string());
            codes.push(c.to_string());
            adj_closes.push(ac);
        }
    }

    let df = DataFrame::new(vec![
        Series::new("Date", &dates),
        Series::new("Code", &codes),
        Series::new("AdjustmentClose", &adj_closes),
    ])?;

    // 3. データ成形（Pivot）
    println!("Pivoting data: transforming from Long to Wide format...");

    // J-Quants V2のヘッダー名に修正: 
    // values: AdjustmentClose (調整後終値), index: Date (日付), columns: Code (銘柄コード)
    let wide_df = pivot(
        &df,
        ["Date"],                  // 行にする列 (index)
        ["Code"],                  // 列に展開する列 (columns)
        Some(["AdjustmentClose"]), // 値にする列 (values)
        false,                     // sort_columns
        Some(PivotAgg::First),     // 重複時の処理
        None                       // separator
    ).map_err(|e| PolarsError::ComputeError(format!("Pivot failed: {}", e).into()))?;

    // 4. 数値キャスト（Date以外の全銘柄列をFloat64に）
    let mut processed_df = wide_df.lazy()
        .with_columns([
            col("*").exclude(["Date"]).cast(DataType::Float64)
        ])
        .collect()?;

    // 5. 保存
    println!("Saving to Parquet (Shape: {:?})", processed_df.shape());
    io::save_parquet(&mut processed_df, output_parquet)?;

    println!("Success! Data is ready for production.");
    Ok(())
}