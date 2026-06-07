use rusqlite::{Connection, OptionalExtension, params};
use chrono::Local;
use polars::prelude::DataFrame;

/// DBの初期化（仮想トレード用テーブルを追加拡張）
pub fn init_db_extended(conn: &Connection) {
    // 既存のOHLCテーブル作成（db::sqlite::init_db を想定）
    crate::db::sqlite::init_db(conn);

    // 1. 現在保有中の仮想ポジションを管理するテーブル
    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS active_positions (
            code TEXT PRIMARY KEY,
            entry_date TEXT,
            entry_price REAL,
            qty INTEGER,
            highest_price REAL, -- トレーリングストップ等で遊べるように最高値も記録
            current_price REAL
        )
        ",
        []
    ).unwrap();

    // 2. 決済が完了したトレードの履歴（勝率計算用）
    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS trade_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            code TEXT,
            entry_date TEXT,
            exit_date TEXT,
            entry_price REAL,
            exit_price REAL,
            qty INTEGER,
            profit_loss REAL,     -- 損益額
            profit_loss_pct REAL  -- 損益率（%）
        )
        ",
        []
    ).unwrap();
}

/// AIが「GO」を出した銘柄を仮想購入（新規ポジション建て）
pub fn record_virtual_buy(
    conn: &Connection,
    code: &str,
    _name: &str, // 将来用、現状DB構成に合わせるため
    price: f64,
    qty: i64,
) -> rusqlite::Result<()> {
    let today_str = Local::now().format("%Y-%m-%d").to_string();

    // すでに保有中の銘柄は二重買いしないように IGNORE
    conn.execute(
        "
        INSERT OR IGNORE INTO active_positions
        (code, entry_date, entry_price, qty, highest_price, current_price)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ",
        params![code, today_str, price, qty, price, price],
    )?;

    println!("📥 [Paper Trade] 仮想購入記録: {} を {}円で {}株", code, price, qty);
    Ok(())
}

/// 保有中ポジションの最新株価更新 ＆ 利確・損切りの自動答え合わせ
pub fn evaluate_and_exit_positions(conn: &Connection) -> rusqlite::Result<()> {
    let today_str = Local::now().format("%Y-%m-%d").to_string();
    
    // 固定ルール（例：利確 +10%, 損切り -5%）
    let target_profit_pct = 0.10;
    let stop_loss_pct = -0.05;

    // 現在の保有ポジションを全件取得
    let mut stmt = conn.prepare("SELECT code, entry_date, entry_price, qty, highest_price FROM active_positions")?;
    let mut rows = stmt.query([])?;

    let mut exits = Vec::new();

    while let Some(row) = rows.next()? {
        let code: String = row.get(0)?;
        let entry_date: String = row.get(1)?;
        let entry_price: f64 = row.get(2)?;
        let qty: i64 = row.get(3)?;
        let mut highest_price: f64 = row.get(4)?;

        // OHLCテーブルから、この銘柄の「最新の終値」を取得する
        let latest_close: Option<f64> = conn.query_row(
            "SELECT close FROM OHLC WHERE code = ?1 ORDER BY date DESC LIMIT 1",
            [code.clone()],
            |r| r.get(0)
        ).optional()?;

        if let Some(current_price) = latest_close {
            // 最高値の更新チェック
            if current_price > highest_price {
                highest_price = current_price;
                conn.execute(
                    "UPDATE active_positions SET highest_price = ?1 WHERE code = ?2",
                    params![highest_price, code],
                )?;
            }

            // 現在の損益率を計算
            let pl_pct = (current_price - entry_price) / entry_price;

            // 利確・損切りラインに達しているかチェック
            if pl_pct >= target_profit_pct || pl_pct <= stop_loss_pct {
                let pl_amount = (current_price - entry_price) * (qty as f64);
                
                // ループ内でのDB削除を避けるため、一旦ベクタに退避
                exits.push((
                    code.clone(), entry_date, today_str.clone(), entry_price, current_price, qty, pl_amount, pl_pct * 100.0
                ));
            } else {
                // 保有継続の場合、現在の株価を更新
                conn.execute(
                    "UPDATE active_positions SET current_price = ?1 WHERE code = ?2",
                    params![current_price, code],
                )?;
            }
        }
    }
    drop(rows);
    drop(stmt);

    // 決済処理（ポジション削除 ＆ 履歴へ挿入）
    for ex in exits {
        println!("💥 [Paper Trade] 決済自動実行: {}（利益率: {:.2}%）", ex.0, ex.7);

        // 1. 履歴へ追加
        conn.execute(
            "
            INSERT INTO trade_history (code, entry_date, exit_date, entry_price, exit_price, qty, profit_loss, profit_loss_pct)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
            params![ex.0.clone(), ex.1, ex.2, ex.3, ex.4, ex.5, ex.6, ex.7],
        )?;

        // 2. 保有から削除
        conn.execute("DELETE FROM active_positions WHERE code = ?1", [ex.0])?;
    }

    Ok(())
}

/// AI判定の通算勝率を計算してテキスト表示する
pub fn log_ai_win_rate(conn: &Connection) -> rusqlite::Result<()> {
    let total_trades: i64 = conn.query_row("SELECT COUNT(*) FROM trade_history", [], |r| r.get(0))?;
    
    if total_trades == 0 {
        println!("📊 [AI Performance] まだ決済されたトレード履歴がありません。");
        return Ok(());
    }

    let win_trades: i64 = conn.query_row("SELECT COUNT(*) FROM trade_history WHERE profit_loss > 0", [], |r| r.get(0))?;
    let win_rate = (win_trades as f64) / (total_trades as f64) * 100.0;
    
    let total_pl: f64 = conn.query_row("SELECT SUM(profit_loss) FROM trade_history", [], |r| r.get(0)).unwrap_or(0.0);

    println!("==================================================");
    println!("📊 【Gemma 3 スクリーニング通算成績】");
    println!("  総トレード数 : {} 回", total_trades);
    println!("  勝率         : {:.2} % （{}勝 / {}敗）", win_rate, win_trades, total_trades - win_trades);
    println!("  通算仮想損益 : {:.0} 円", total_pl);
    println!("==================================================");

    Ok(())
}

// TUI互換性のために残す（必要に応じてSQLite版に置き換え）
pub fn load_portfolio() -> Result<DataFrame, Box<dyn std::error::Error>> {
    // 互換性のためのスタブ。実際にはSQLiteからDataFrameに変換するなどの処理が必要
    Ok(DataFrame::empty())
}
