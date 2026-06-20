use rusqlite::{Connection, OptionalExtension, params};
use chrono::Local;
use polars::prelude::DataFrame;

/// DBの初期化（仮想トレード用テーブルを追加拡張）
pub fn init_db_extended(conn: &Connection) -> rusqlite::Result<()> {
    // 既存のOHLCテーブル作成（db::sqlite::init_db を想定）
    crate::db::sqlite::init_db(conn)?;

    // 1. 現在保有中の仮想ポジションを管理するテーブル
    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS active_positions (
            code TEXT PRIMARY KEY,
            entry_date TEXT,
            entry_price REAL,
            qty INTEGER,
            highest_price REAL, -- トレーリングストップ等で遊べるように最高値も記録
            current_price REAL,
            status TEXT DEFAULT 'HOLDING',
            holding_days INTEGER DEFAULT 0,
            exit_reason TEXT
        )
        ",
        [],
    )?;

    // 既存のDBでカラムが不足している場合に備えて、ALTER TABLEを安全に実行する
    let _ = conn.execute("ALTER TABLE active_positions ADD COLUMN status TEXT DEFAULT 'HOLDING'", []);
    let _ = conn.execute("ALTER TABLE active_positions ADD COLUMN holding_days INTEGER DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE active_positions ADD COLUMN exit_reason TEXT", []);

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
        [],
    )?;
    Ok(())
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

    // ステータスを PENDING_BUY として挿入（明朝始値で約定）
    conn.execute(
        "
        INSERT OR IGNORE INTO active_positions
        (code, entry_date, entry_price, qty, highest_price, current_price, status, holding_days)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'PENDING_BUY', 0)
        ",
        params![code, today_str, price, qty, price, price],
    )?;

    println!("📥 [Paper Trade] 仮想購入予約（PENDING_BUY）を追加: {} (推定価格: {}円) {}株", code, price, qty);
    Ok(())
}

/// 1. 前日の予約（PENDING）を本日の始値(Open)ベースで約定させる関数
pub async fn execute_pending_orders(conn: &Connection) -> rusqlite::Result<()> {
    let today_str = Local::now().format("%Y-%m-%d").to_string();

    // ① PENDING_BUY の約定処理
    let mut stmt = conn.prepare("SELECT code, qty FROM active_positions WHERE status = 'PENDING_BUY'")?;
    let mut rows = stmt.query([])?;
    let mut filled_buys = Vec::new();

    while let Some(row) = rows.next()? {
        let code: String = row.get(0)?;
        let qty: i64 = row.get(1)?;

        // 最新の始値を取得
        let latest_open: Option<f64> = conn.query_row(
            "SELECT open FROM OHLC WHERE code = ?1 ORDER BY date DESC LIMIT 1",
            [code.clone()],
            |r| r.get(0)
        ).optional()?;

        if let Some(open_price) = latest_open {
            filled_buys.push((code, open_price, qty));
        }
    }
    drop(rows);
    drop(stmt);

    for (code, open_price, qty) in filled_buys {
        println!("🛒 [Paper Trade] PENDING_BUY 買い約定実行: {} | 価格: {}円", code, open_price);
        conn.execute(
            "UPDATE active_positions SET status = 'HOLDING', entry_price = ?1, highest_price = ?2, current_price = ?3, holding_days = 0, entry_date = ?4 WHERE code = ?5",
            params![open_price, open_price, open_price, today_str, code],
        )?;

        // Discord通知
        let _ = crate::api::discord::notify_order_execution(&code, true, open_price, qty, None, None).await;
    }

    // ② PENDING_SELL の約定処理
    let mut stmt = conn.prepare("SELECT code, entry_date, entry_price, qty, exit_reason FROM active_positions WHERE status = 'PENDING_SELL'")?;
    let mut rows = stmt.query([])?;
    let mut filled_sells = Vec::new();

    while let Some(row) = rows.next()? {
        let code: String = row.get(0)?;
        let entry_date: String = row.get(1)?;
        let entry_price: f64 = row.get(2)?;
        let qty: i64 = row.get(3)?;
        let exit_reason: String = row.get(4).unwrap_or_else(|_| "不明な理由".to_string());

        // 最新の始値を取得
        let latest_open: Option<f64> = conn.query_row(
            "SELECT open FROM OHLC WHERE code = ?1 ORDER BY date DESC LIMIT 1",
            [code.clone()],
            |r| r.get(0)
        ).optional()?;

        if let Some(open_price) = latest_open {
            filled_sells.push((code, entry_date, entry_price, open_price, qty, exit_reason));
        }
    }
    drop(rows);
    drop(stmt);

    for (code, entry_date, entry_price, open_price, qty, exit_reason) in filled_sells {
        let pl_amount = (open_price - entry_price) * (qty as f64);
        let pl_pct = ((open_price - entry_price) / entry_price) * 100.0;

        println!("💰 [Paper Trade] PENDING_SELL 売り約定実行: {} | 価格: {}円 (損益: {}円, {:.2}%)", code, open_price, pl_amount, pl_pct);

        // 1. 履歴へ追加
        conn.execute(
            "
            INSERT INTO trade_history (code, entry_date, exit_date, entry_price, exit_price, qty, profit_loss, profit_loss_pct)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
            params![code.clone(), entry_date, today_str.clone(), entry_price, open_price, qty, pl_amount, pl_pct],
        )?;

        // 2. 保有から削除
        conn.execute("DELETE FROM active_positions WHERE code = ?1", [code.clone()])?;

        // Discord通知
        let _ = crate::api::discord::notify_order_execution(&code, false, open_price, qty, Some(pl_amount), Some(pl_pct)).await;
        let _ = crate::api::discord::notify_trade_exit(&code, &code, entry_price, open_price, pl_pct, &exit_reason).await;
    }

    Ok(())
}

/// 保有中ポジションの最新株価更新 ＆ 利確・損切りの自動答え合わせ（PENDING_SELL への移行判定）
pub async fn evaluate_and_exit_positions(conn: &Connection) -> rusqlite::Result<()> {
    // ⚙️ トレーリングストップのパラメータ設定
    let trailing_trigger_pct = 0.05; // 5%以上の含み益でトレーリング発動
    let trailing_drop_pct = 0.03;    // 最高値から3%下落したら利確
    let absolute_stop_loss_pct = -0.05; // 購入価格から5%下落で絶対損切り

    // 1. まず HOLDING 中の全ポジションの保有日数をインクリメント
    conn.execute(
        "UPDATE active_positions SET holding_days = holding_days + 1 WHERE status = 'HOLDING'",
        [],
    )?;

    // 2. HOLDING 中のポジションを全件取得して評価
    let mut stmt = conn.prepare(
        "SELECT code, entry_date, entry_price, qty, highest_price, holding_days FROM active_positions WHERE status = 'HOLDING'"
    )?;
    let mut rows = stmt.query([])?;

    let mut pending_exits = Vec::new();

    while let Some(row) = rows.next()? {
        let code: String = row.get(0)?;
        let _entry_date: String = row.get(1)?;
        let entry_price: f64 = row.get(2)?;
        let _qty: i64 = row.get(3)?;
        let mut highest_price: f64 = row.get(4)?;
        let holding_days: i64 = row.get(5)?;

        // OHLCテーブルから、この銘柄の「最新の終値」を取得
        let latest_close: Option<f64> = conn.query_row(
            "SELECT close FROM OHLC WHERE code = ?1 ORDER BY date DESC LIMIT 1",
            [code.clone()],
            |r| r.get(0)
        ).optional()?;

        if let Some(current_price) = latest_close {
            // ① 最高値の更新チェック
            if current_price > highest_price {
                highest_price = current_price;
                conn.execute(
                    "UPDATE active_positions SET highest_price = ?1 WHERE code = ?2",
                    params![highest_price, code],
                )?;
            }

            // ② 各種損益率の計算
            let current_pl_pct = (current_price - entry_price) / entry_price; // 購入原価からの損益率
            let max_gain_pct = (highest_price - entry_price) / entry_price;   // これまでの最大利益率
            let drop_from_peak_pct = (highest_price - current_price) / highest_price; // 最高値からの下落率

            let mut is_exit = false;
            let mut exit_reason = String::new();

            // ③ 決済判定ロジック
            if current_pl_pct <= absolute_stop_loss_pct {
                // ① 絶対損切りラインに接触
                is_exit = true;
                exit_reason = "絶対損切り(-5%)".to_string();
            } else if max_gain_pct >= trailing_trigger_pct && drop_from_peak_pct >= trailing_drop_pct {
                // ② トレーリングストップ発動（5%以上上昇後、最高値から3%下落）
                is_exit = true;
                exit_reason = format!("トレーリングストップ利確(ピークから-{:.1}%)", trailing_drop_pct * 100.0);
            } else if holding_days >= 10 {
                // ③ 10日タイムアウト制限
                is_exit = true;
                exit_reason = "10日タイムアウト制限".to_string();
            }

            // ④ 決済判定に該当した場合は PENDING_SELL に変更（明朝始値で約定）
            if is_exit {
                pending_exits.push((code.clone(), exit_reason));
            } else {
                conn.execute(
                    "UPDATE active_positions SET current_price = ?1 WHERE code = ?2",
                    params![current_price, code],
                )?;
            }
        }
    }
    drop(rows);
    drop(stmt);

    // PENDING_SELL へのステータス変更を実行
    for (code, reason) in pending_exits {
        println!("⏳ [Paper Trade] 決済シグナル検知: {} -> PENDING_SELL へ変更（理由: {}）", code, reason);
        conn.execute(
            "UPDATE active_positions SET status = 'PENDING_SELL', exit_reason = ?1 WHERE code = ?2",
            params![reason, code],
        )?;

        // PENDING_SELL への移行検知を Discord 通知する
        let _ = crate::api::discord::notify_discord(
            &code,
            0.0,
            &format!("⏳ 決済準備 (PENDING_SELL): 明朝始値で売却予約されます。理由: {}", reason)
        ).await;
    }

    Ok(())
}

/// AI判定の通算勝率を計算してテキスト表示する
pub async fn log_ai_win_rate(conn: &Connection) -> rusqlite::Result<()> {
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

    // 💡 追記: 通算成績をDiscordへ通知
    let _ = crate::api::discord::notify_performance_report(total_trades, win_trades, win_rate, total_pl).await;

    Ok(())
}

// TUI互換性のために残す（必要に応じてSQLite版に置き換え）
pub fn load_portfolio() -> Result<DataFrame, Box<dyn std::error::Error>> {
    // 互換性のためのスタブ。実際にはSQLiteからDataFrameに変換するなどの処理が必要
    Ok(DataFrame::empty())
}
