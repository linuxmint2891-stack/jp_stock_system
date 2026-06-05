use jp_stock_system::paper_trade;
use anyhow::Result;

fn main() -> Result<()> {
    println!("=========================================");
    println!(" 📊 仮想ポートフォリオ（ペーパートレード）");
    println!("=========================================");

    // 1. 市場データがある場合は、最新価格で評価損益を更新
    let market_data_path = "data/processed_market_data.parquet";
    if std::path::Path::new(market_data_path).exists() {
        if let Err(e) = paper_trade::update_portfolio_prices(market_data_path) {
            eprintln!("⚠️ 評価損益の更新に失敗しました: {}", e);
        }
    }

    // 2. 台帳の読み込み
    let df = paper_trade::load_portfolio()
        .map_err(|e| anyhow::anyhow!("台帳の読み込みに失敗: {}", e))?;

    if df.height() == 0 {
        println!(" (現在、保有している仮想銘柄はありません)");
        println!("=========================================");
        return Ok(());
    }

    // 3. 表示
    println!(" 銘柄コード | 銘柄名       | 購入日     | 購入単価 | 現在値  | 数量 | 評価損益");
    println!("-------------------------------------------------------------------------");

    let codes = df.column("code")?.str()?;
    let names = df.column("name")?.str()?;
    let entry_dates = df.column("entry_date")?.str()?;
    let entry_prices = df.column("entry_price")?.f64()?;
    let current_prices = df.column("current_price")?.f64()?;
    let qtys = df.column("qty")?.i64()?;
    let pls = df.column("profit_loss")?.f64()?;

    let mut total_pl = 0.0;
    let mut wins = 0;

    for i in 0..df.height() {
        let code = codes.get(i).unwrap_or("");
        let name = names.get(i).unwrap_or("");
        let date = entry_dates.get(i).unwrap_or("");
        let e_price = entry_prices.get(i).unwrap_or(0.0);
        let c_price = current_prices.get(i).unwrap_or(0.0);
        let qty = qtys.get(i).unwrap_or(0);
        let pl = pls.get(i).unwrap_or(0.0);

        total_pl += pl;
        if pl > 0.0 { wins += 1; }

        println!(" {:<10} | {:<12} | {:<10} | {:>8.1} | {:>7.1} | {:>4} | {:>+9.1}円",
            code, name, date, e_price, c_price, qty, pl);
    }

    println!("-------------------------------------------------------------------------");
    let win_rate = (wins as f64 / df.height() as f64) * 100.0;
    println!(" 【合計含み損益】: {:+9.1}円 (勝率: {:.1}%)", total_pl, win_rate);
    println!("=========================================");

    Ok(())
}
