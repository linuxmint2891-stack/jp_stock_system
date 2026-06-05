use polars::prelude::*;
use jp_stock_system::alpha::alpha_a;

#[test]
fn test_alpha_a_computation() -> PolarsResult<()> {
    // 1. テストデータの準備
    let df = df!(
        "Date" => &["2024-01-01", "2024-01-02", "2024-01-03", "2024-01-04", "2024-01-05"],
        "Code" => &["1001", "1001", "1001", "1001", "1001"],
        "AdjC" => &[100.0, 110.0, 120.0, 130.0, 140.0]
    )?;

    let lf = df.lazy();

    // 2. Alpha A の計算実行
    let result_df = alpha_a::compute(lf).collect()?;

    // 3. 検証
    assert!(result_df.column("alpha_a").is_ok());
    
    let alpha_a_vals = result_df.column("alpha_a")?.f64()?;
    
    // 最初の要素: (100 / 100) - 1 = 0
    assert_eq!(alpha_a_vals.get(0), Some(0.0));
    
    // 5番目の要素: 140 / ((100+110+120+130+140)/5) - 1 = 140/120 - 1 = 1.1666... - 1 = 0.1666...
    let last_val = alpha_a_vals.get(4).unwrap();
    assert!((last_val - 0.16666666666666666).abs() < 1e-10);

    Ok(())
}
