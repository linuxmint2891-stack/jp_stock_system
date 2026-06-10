use rusqlite::Connection;
use anyhow::Result;
use jp_stock_system::paper_trade;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=========================================");
    println!(" 📊 仮想ポートフォリオ（ペーパートレード）");
    println!("=========================================");

    // SQLite接続の準備
    let conn = Connection::open("stocks.db")?;
    paper_trade::init_db_extended(&conn)?;

    // 1. 市場データがある場合は、最新価格で評価損益を更新
    let market_data_path = "data/processed_market_data.parquet";
    if std::path::Path::new(market_data_path).exists() {
        println!("📈 最新価格で評価を更新中...");
        if let Err(e) = paper_trade::evaluate_and_exit_positions(&conn).await {
            eprintln!("⚠️ 評価損益の更新に失敗しました: {}", e);
        }
    }

    // 2. 現在のポジション表示
    println!("\n【📈 現在保有中のポジション】");
    println!(" 銘柄コード | 購入日     | 購入単価 | 現在値  | 数量 | 評価損益 (最高値)");
    println!("-------------------------------------------------------------------------");

    let mut stmt = conn.prepare("SELECT code, entry_date, entry_price, current_price, qty, highest_price FROM active_positions")?;
    let mut rows = stmt.query([])?;

    let mut has_positions = false;
    let mut total_unrealized_pl = 0.0;

    while let Some(row) = rows.next()? {
        has_positions = true;
        let code: String = row.get(0)?;
        let date: String = row.get(1)?;
        let e_price: f64 = row.get(2)?;
        let c_price: f64 = row.get(3)?;
        let qty: i64 = row.get(4)?;
        let h_price: f64 = row.get(5)?;
        
        let pl = (c_price - e_price) * (qty as f64);
        total_unrealized_pl += pl;

        println!(" {:<10} | {:<10} | {:>8.1} | {:>7.1} | {:>4} | {:>+9.1}円 ({:>7.1})",
            code, date, e_price, c_price, qty, pl, h_price);
    }

    if !has_positions {
        println!(" (現在、保有している仮想銘柄はありません)");
    }
    println!("-------------------------------------------------------------------------");
    println!(" 【合計含み損益】: {:+9.1}円", total_unrealized_pl);

    // 3. 決済履歴の表示 (最新10件)
    println!("\n【📜 直近の決済履歴】");
    println!(" 銘柄コード | 購入日     | 決済日     | 購入単価 | 決済単価 | 数量 | 確定損益");
    println!("-------------------------------------------------------------------------");

    let mut stmt_hist = conn.prepare("SELECT code, entry_date, exit_date, entry_price, exit_price, qty, profit_loss FROM trade_history ORDER BY id DESC LIMIT 10")?;
    let mut rows_hist = stmt_hist.query([])?;

    let mut has_history = false;
    while let Some(row) = rows_hist.next()? {
        has_history = true;
        let code: String = row.get(0)?;
        let e_date: String = row.get(1)?;
        let x_date: String = row.get(2)?;
        let e_price: f64 = row.get(3)?;
        let x_price: f64 = row.get(4)?;
        let qty: i64 = row.get(5)?;
        let pl: f64 = row.get(6)?;

        println!(" {:<10} | {:<10} | {:<10} | {:>8.1} | {:>8.1} | {:>4} | {:>+9.1}円",
            code, e_date, x_date, e_price, x_price, qty, pl);
    }

    if !has_history {
        println!(" (まだ決済履歴はありません)");
    }
    println!("-------------------------------------------------------------------------");

    // 4. 通算成績の表示
    if let Err(e) = paper_trade::log_ai_win_rate(&conn).await {
        eprintln!("⚠️ 成績の取得に失敗しました: {}", e);
    }

    println!("=========================================");

    Ok(())
}
