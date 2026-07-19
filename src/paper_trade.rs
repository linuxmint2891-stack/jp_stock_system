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
            name TEXT,
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
    let _ = conn.execute("ALTER TABLE active_positions ADD COLUMN name TEXT", []);

    // 2. 決済が完了したトレードの履歴（勝率計算用）
    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS trade_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            code TEXT,
            name TEXT,
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
    let _ = conn.execute("ALTER TABLE trade_history ADD COLUMN name TEXT", []);
    Ok(())
}

/// AIが「GO」を出した銘柄を仮想購入（新規ポジション建て）
pub fn record_virtual_buy(
    conn: &Connection,
    code: &str,
    name: &str,
    price: f64,
    qty: i64,
) -> rusqlite::Result<()> {
    let today_str = Local::now().format("%Y-%m-%d").to_string();

    // ステータスを PENDING_BUY として挿入（明朝始値で約定）
    conn.execute(
        "
        INSERT OR IGNORE INTO active_positions
        (code, name, entry_date, entry_price, qty, highest_price, current_price, status, holding_days)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'PENDING_BUY', 0)
        ",
        params![code, name, today_str, price, qty, price, price],
    )?;

    println!("📥 [Paper Trade] 仮想購入予約（PENDING_BUY）を追加: {} ({}) (推定価格: {}円) {}株", code, name, price, qty);
    Ok(())
}

/// 1. 前日の予約（PENDING）を本日の始値(Open)ベースで約定させる関数
pub async fn execute_pending_orders(conn: &Connection) -> rusqlite::Result<()> {
    let today_str = Local::now().format("%Y-%m-%d").to_string();

    // ① PENDING_BUY の約定処理
    let mut stmt = conn.prepare("SELECT code, name, qty FROM active_positions WHERE status = 'PENDING_BUY'")?;
    let mut rows = stmt.query([])?;
    let mut filled_buys = Vec::new();

    while let Some(row) = rows.next()? {
        let code: String = row.get(0)?;
        let name: String = row.get(1).unwrap_or_else(|_| "".to_string());
        let qty: i64 = row.get(2)?;

        // 最新の始値を取得
        let latest_open: Option<f64> = conn.query_row(
            "SELECT open FROM OHLC WHERE code = ?1 ORDER BY date DESC LIMIT 1",
            [code.clone()],
            |r| r.get(0)
        ).optional()?;

        if let Some(open_price) = latest_open {
            filled_buys.push((code, name, open_price, qty));
        }
    }
    drop(rows);
    drop(stmt);

    for (code, name, open_price, qty) in filled_buys {
        println!("🛒 [Paper Trade] PENDING_BUY 買い約定実行: {} ({}) | 価格: {}円", code, name, open_price);
        conn.execute(
            "UPDATE active_positions SET status = 'HOLDING', entry_price = ?1, highest_price = ?2, current_price = ?3, holding_days = 0, entry_date = ?4 WHERE code = ?5",
            params![open_price, open_price, open_price, today_str, code],
        )?;

        // Discord通知
        let _ = crate::api::discord::notify_order_execution(&code, true, open_price, qty, None, None).await;
    }

    // ② PENDING_SELL の約定処理
    let mut stmt = conn.prepare("SELECT code, name, entry_date, entry_price, qty, exit_reason FROM active_positions WHERE status = 'PENDING_SELL'")?;
    let mut rows = stmt.query([])?;
    let mut filled_sells = Vec::new();

    while let Some(row) = rows.next()? {
        let code: String = row.get(0)?;
        let name: String = row.get(1).unwrap_or_else(|_| "".to_string());
        let entry_date: String = row.get(2)?;
        let entry_price: f64 = row.get(3)?;
        let qty: i64 = row.get(4)?;
        let exit_reason: String = row.get(5).unwrap_or_else(|_| "不明な理由".to_string());

        // 最新の始値を取得
        let latest_open: Option<f64> = conn.query_row(
            "SELECT open FROM OHLC WHERE code = ?1 ORDER BY date DESC LIMIT 1",
            [code.clone()],
            |r| r.get(0)
        ).optional()?;

        if let Some(open_price) = latest_open {
            filled_sells.push((code, name, entry_date, entry_price, open_price, qty, exit_reason));
        }
    }
    drop(rows);
    drop(stmt);

    for (code, name, entry_date, entry_price, open_price, qty, exit_reason) in filled_sells {
        let pl_amount = (open_price - entry_price) * (qty as f64);
        let pl_pct = ((open_price - entry_price) / entry_price) * 100.0;

        println!("💰 [Paper Trade] PENDING_SELL 売り約定実行: {} | 価格: {}円 (損益: {}円, {:.2}%)", code, open_price, pl_amount, pl_pct);

        // 1. 履歴へ追加
        conn.execute(
            "
            INSERT INTO trade_history (code, name, entry_date, exit_date, entry_price, exit_price, qty, profit_loss, profit_loss_pct)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ",
            params![code.clone(), name.clone(), entry_date, today_str.clone(), entry_price, open_price, qty, pl_amount, pl_pct],
        )?;

        // 2. 保有から削除
        conn.execute("DELETE FROM active_positions WHERE code = ?1", [code.clone()])?;

        // Discord通知
        let _ = crate::api::discord::notify_order_execution(&code, false, open_price, qty, Some(pl_amount), Some(pl_pct)).await;
        let _ = crate::api::discord::notify_trade_exit(&code, &name, entry_price, open_price, pl_pct, &exit_reason).await;
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
    let today_str = Local::now().format("%Y-%m-%d").to_string();

    // 1. 保有中ポジションの含み損益の計算
    let mut stmt = conn.prepare("SELECT code, name, entry_price, current_price, qty FROM active_positions")?;
    let mut rows = stmt.query([])?;
    let mut positions = Vec::new();
    let mut total_unrealized_pl = 0.0;

    while let Some(row) = rows.next()? {
        let code: String = row.get(0)?;
        let name: String = row.get(1).unwrap_or_else(|_| "".to_string());
        let entry_price: f64 = row.get(2)?;
        let current_price: f64 = row.get(3)?;
        let qty: i64 = row.get(4)?;

        let pl = (current_price - entry_price) * (qty as f64);
        let pl_pct = ((current_price - entry_price) / entry_price) * 100.0;
        total_unrealized_pl += pl;

        positions.push((code, name, entry_price, current_price, qty, pl, pl_pct));
    }
    drop(rows);
    drop(stmt);

    // 2. 過去の決済済データの集計（確定損益・勝率・プロフィットファクター）
    let total_trades: i64 = conn.query_row("SELECT COUNT(*) FROM trade_history", [], |r| r.get(0))?;

    let mut win_trades = 0;
    let mut win_rate = 0.0;
    let mut total_realized_pl = 0.0;
    let mut profit_factor = 0.0;

    if total_trades > 0 {
        win_trades = conn.query_row("SELECT COUNT(*) FROM trade_history WHERE profit_loss > 0", [], |r| r.get(0))?;
        win_rate = (win_trades as f64) / (total_trades as f64) * 100.0;
        total_realized_pl = conn.query_row("SELECT COALESCE(SUM(profit_loss), 0.0) FROM trade_history", [], |r| r.get(0))?;

        let total_profit: f64 = conn.query_row("SELECT COALESCE(SUM(profit_loss), 0.0) FROM trade_history WHERE profit_loss > 0", [], |r| r.get(0))?;
        let total_loss: f64 = conn.query_row("SELECT COALESCE(SUM(profit_loss), 0.0) FROM trade_history WHERE profit_loss < 0", [], |r| r.get(0))?;

        if total_loss.abs() > 0.0 {
            profit_factor = total_profit / total_loss.abs();
        } else if total_profit > 0.0 {
            profit_factor = 99.99; // 損失がなく利益がある場合
        }
    }

    println!("==================================================");
    println!("📊 【AIペーパートレード 運用パフォーマンス報告】");
    println!("==================================================");
    println!("📅 集計日: {}", today_str);
    println!("\n現時点での保有ポジション (含み損益):");
    if positions.is_empty() {
        println!("  • なし");
    } else {
        for (code, name, entry_price, current_price, _qty, pl, pl_pct) in &positions {
            println!("  • {} {}\n    購入: {:.0}円 -> 現在: {:.0}円 ({:+.2}%) | 評価損益: {:+.0}円",
                code, name, entry_price, current_price, pl_pct, pl);
        }
    }
    println!("-----------------------------------------");
    println!("💰 資産状況サマリー:");
    println!("  • 総含み損益 (評価損益) : {:+.0} 円", total_unrealized_pl);
    println!("  • 通算確定損益 (実現損益) : {:+.0} 円", total_realized_pl);
    println!("\n📈 AIスコア運用の通算成績:");
    println!("  • 総トレード回数 : {} 回", total_trades);
    println!("  • 勝敗 : {}勝 {}敗 (勝率: {:.1}%)", win_trades, total_trades - win_trades, win_rate);
    println!("  • プロフィットファクター : {:.2}", profit_factor);
    println!("==================================================");

    // 💡 追記: 通算成績をDiscordへ通知
    let _ = crate::api::discord::notify_portfolio_summary_report(
        &today_str,
        &positions,
        total_unrealized_pl,
        total_realized_pl,
        total_trades,
        win_trades,
        win_rate,
        profit_factor,
    ).await;

    // 💡 追記: 詳細レポートをファイル（portfolio_report.txt）に書き出し、Discordに添付送信する
    if let Ok(report_str) = generate_portfolio_report_string(conn).await {
        let report_path = "data/portfolio_report.txt";
        let _ = std::fs::create_dir_all("data");
        if std::fs::write(report_path, report_str).is_ok() {
            if let Ok(webhook_url) = std::env::var("DISCORD_WEBHOOK_URL") {
                let summary_msg = "📊 本日のポートフォリオおよび取引履歴の詳細レポート（ファイルログ）です。";
                let _ = crate::api::discord::send_portfolio_file_to_discord(&webhook_url, report_path, summary_msg).await;
            }
        }
    }

    Ok(())
}

/// ポートフォリオと取引履歴をテキスト形式のレポートとして生成する
pub async fn generate_portfolio_report_string(conn: &Connection) -> rusqlite::Result<String> {
    let mut report = String::new();

    report.push_str("=========================================\n");
    report.push_str(" 📊 仮想ポートフォリオ 詳細レポート\n");
    report.push_str("=========================================\n");
    report.push_str(&format!("📅 生成日時: {}\n\n", Local::now().format("%Y-%m-%d %H:%M:%S")));

    // 1. 保有中ポジションの表示
    report.push_str("【📈 現在保有中のポジション】\n");
    report.push_str(" 銘柄コード | 銘柄名               | 購入日     | 購入単価 | 現在値  | 数量 | 評価損益 (最高値)\n");
    report.push_str("-------------------------------------------------------------------------------------------------\n");

    let mut stmt = conn.prepare("SELECT code, name, entry_date, entry_price, current_price, qty, highest_price FROM active_positions")?;
    let mut rows = stmt.query([])?;
    let mut has_positions = false;
    let mut total_unrealized_pl = 0.0;

    while let Some(row) = rows.next()? {
        has_positions = true;
        let code: String = row.get(0)?;
        let name: String = row.get(1).unwrap_or_else(|_| "".to_string());
        let date: String = row.get(2)?;
        let e_price: f64 = row.get(3)?;
        let c_price: f64 = row.get(4)?;
        let qty: i64 = row.get(5)?;
        let h_price: f64 = row.get(6)?;
        
        let pl = (c_price - e_price) * (qty as f64);
        total_unrealized_pl += pl;

        report.push_str(&format!(" {:<10} | {:<20} | {:<10} | {:>8.1} | {:>7.1} | {:>4} | {:>+9.1}円 ({:>7.1})\n",
            code, name, date, e_price, c_price, qty, pl, h_price));
    }

    if !has_positions {
        report.push_str(" (現在、保有している仮想銘柄はありません)\n");
    }
    report.push_str("-------------------------------------------------------------------------------------------------\n");
    report.push_str(&format!(" 【合計含み損益】: {:+9.1}円\n\n", total_unrealized_pl));

    // 2. 決済履歴の表示 (最新50件)
    report.push_str("【📜 直近の決済履歴】\n");
    report.push_str(" 銘柄コード | 銘柄名               | 購入日     | 決済日     | 購入単価 | 決済単価 | 数量 | 確定損益\n");
    report.push_str("-------------------------------------------------------------------------------------------------\n");

    let mut stmt_hist = conn.prepare("SELECT code, name, entry_date, exit_date, entry_price, exit_price, qty, profit_loss FROM trade_history ORDER BY id DESC LIMIT 50")?;
    let mut rows_hist = stmt_hist.query([])?;
    let mut has_history = false;

    while let Some(row) = rows_hist.next()? {
        has_history = true;
        let code: String = row.get(0)?;
        let name: String = row.get(1).unwrap_or_else(|_| "".to_string());
        let e_date: String = row.get(2)?;
        let x_date: String = row.get(3)?;
        let e_price: f64 = row.get(4)?;
        let x_price: f64 = row.get(5)?;
        let qty: i64 = row.get(6)?;
        let pl: f64 = row.get(7)?;

        report.push_str(&format!(" {:<10} | {:<20} | {:<10} | {:<10} | {:>8.1} | {:>8.1} | {:>4} | {:>+9.1}円\n",
            code, name, e_date, x_date, e_price, x_price, qty, pl));
    }

    if !has_history {
        report.push_str(" (まだ決済履歴はありません)\n");
    }
    report.push_str("-------------------------------------------------------------------------------------------------\n\n");

    // 3. 通算成績
    let total_trades: i64 = conn.query_row("SELECT COUNT(*) FROM trade_history", [], |r| r.get(0))?;
    let mut win_trades = 0;
    let mut win_rate = 0.0;
    let mut total_realized_pl = 0.0;
    let mut profit_factor = 0.0;

    if total_trades > 0 {
        win_trades = conn.query_row("SELECT COUNT(*) FROM trade_history WHERE profit_loss > 0", [], |r| r.get(0))?;
        win_rate = (win_trades as f64) / (total_trades as f64) * 100.0;
        total_realized_pl = conn.query_row("SELECT COALESCE(SUM(profit_loss), 0.0) FROM trade_history", [], |r| r.get(0))?;

        let total_profit: f64 = conn.query_row("SELECT COALESCE(SUM(profit_loss), 0.0) FROM trade_history WHERE profit_loss > 0", [], |r| r.get(0))?;
        let total_loss: f64 = conn.query_row("SELECT COALESCE(SUM(profit_loss), 0.0) FROM trade_history WHERE profit_loss < 0", [], |r| r.get(0))?;

        if total_loss.abs() > 0.0 {
            profit_factor = total_profit / total_loss.abs();
        } else if total_profit > 0.0 {
            profit_factor = 99.99;
        }
    }

    report.push_str("==================================================\n");
    report.push_str("📊 【AIスコア運用の通算成績】\n");
    report.push_str(&format!("  総トレード数 : {} 回\n", total_trades));
    report.push_str(&format!("  勝率         : {:.2} % （{}勝 / {}敗）\n", win_rate, win_trades, total_trades - win_trades));
    report.push_str(&format!("  通算確定損益 : {:.0} 円\n", total_realized_pl));
    report.push_str(&format!("  プロフィットファクター : {:.2}\n", profit_factor));
    report.push_str("==================================================\n");

    Ok(report)
}

// TUI互換性のために残す（必要に応じてSQLite版に置き換え）
pub fn load_portfolio() -> Result<DataFrame, Box<dyn std::error::Error>> {
    // 互換性のためのスタブ。実際にはSQLiteからDataFrameに変換するなどの処理が必要
    Ok(DataFrame::empty())
}
