use rusqlite::{Connection, OptionalExtension, params};
use chrono::Local;
use polars::prelude::*;
use std::collections::HashMap;
use crate::utils::settings::ExitStrategySettings;

pub struct MarketPriceSnapshot {
    pub date: String,
    pub prices: HashMap<String, f64>,
    /// 銘柄別に採用した終値の日付。価格の鮮度検証に使用する。
    pub price_dates: HashMap<String, String>,
    /// 終値ベースの平均絶対日次変動率（ATR相当、0.01 = 1%）。
    pub atr_percent: HashMap<String, f64>,
}

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
            exit_reason TEXT,
            last_evaluated_date TEXT
        )
        ",
        [],
    )?;

    // 既存のDBでカラムが不足している場合に備えて、ALTER TABLEを安全に実行する
    let _ = conn.execute("ALTER TABLE active_positions ADD COLUMN status TEXT DEFAULT 'HOLDING'", []);
    let _ = conn.execute("ALTER TABLE active_positions ADD COLUMN holding_days INTEGER DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE active_positions ADD COLUMN exit_reason TEXT", []);
    let _ = conn.execute("ALTER TABLE active_positions ADD COLUMN name TEXT", []);
    let _ = conn.execute("ALTER TABLE active_positions ADD COLUMN last_evaluated_date TEXT", []);

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

    // 手動・ペーパートレードでは、AIの購入提案価格を約定価格として即時に保有扱いにする。
    // 実際の証券会社で約定した場合は、実約定価格で別途調整する。
    conn.execute(
        "
        INSERT OR IGNORE INTO active_positions
        (code, name, entry_date, entry_price, qty, highest_price, current_price, status, holding_days)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'HOLDING', 0)
        ",
        params![code, name, today_str, price, qty, price, price],
    )?;

    println!("📥 [Paper Trade] 仮想保有（HOLDING）を追加: {} ({}) (記録価格: {}円) {}株", code, name, price, qty);
    Ok(())
}

/// 旧バージョンで作成され、約定待ちのまま残ったペーパートレード注文を保有状態へ移行する。
/// この移行は PENDING_BUY の行にのみ作用し、新規注文は record_virtual_buy で直接 HOLDING となる。
pub fn activate_legacy_pending_buys(conn: &Connection) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE active_positions SET status = 'HOLDING' WHERE status = 'PENDING_BUY'",
        [],
    )
}

/// 種銭と予約・保有済み金額から、次の小口購入に使える株数を計算する。
/// 単元未満株を前提に、1株単位で切り捨てる。
pub fn calculate_fractional_buy_qty(
    conn: &Connection,
    total_budget: u64,
    position_budget: u64,
    price: f64,
) -> rusqlite::Result<Option<(i64, f64)>> {
    if !price.is_finite() || price <= 0.0 {
        return Ok(None);
    }

    let reserved: f64 = conn.query_row(
        "
        SELECT COALESCE(SUM(entry_price * qty), 0.0)
        FROM active_positions
        WHERE status IN ('PENDING_BUY', 'HOLDING', 'PENDING_SELL')
        ",
        [],
        |row| row.get(0),
    )?;
    let remaining_budget = (total_budget as f64 - reserved).max(0.0);
    let allocated_budget = remaining_budget.min(position_budget as f64);
    let qty = (allocated_budget / price).floor() as i64;

    if qty < 1 {
        return Ok(None);
    }
    Ok(Some((qty, qty as f64 * price)))
}

/// Parquet の銘柄別最新終値を読み込む。
/// ポートフォリオ評価の価格ソースを日次同期データへ統一するために使用する。
pub fn latest_prices_from_parquet(path: &str) -> PolarsResult<(String, HashMap<String, f64>)> {
    let snapshot = market_price_snapshot_from_parquet(path, 14)?;
    Ok((snapshot.date, snapshot.prices))
}

/// 銘柄別の最新終値と、直近N営業日の終値ベースATR相当値をParquetから作成する。
/// OHLCの高値・安値を持たないデータ形式のため、平均絶対日次変動率を使用する。
pub fn market_price_snapshot_from_parquet(
    path: &str,
    atr_lookback_days: usize,
) -> PolarsResult<MarketPriceSnapshot> {
    // J-Quants由来の5桁コード（例: 67680）とYahoo由来の4桁コード（6768）を
    // 同一銘柄として扱う。正規化より後の全計算を4桁コード単位で行うことで、
    // 古い5桁履歴が新しいYahoo終値を上書きすることを防ぐ。
    let market_lf = LazyFrame::scan_parquet(path, Default::default())?.select([
        col("Date"),
        col("Code")
            .cast(DataType::String)
            .str()
            .slice(lit(0), lit(4))
            .alias("Code"),
        col("AdjC"),
    ]);
    let daily_return = col("AdjC") / col("AdjC").shift(lit(1)) - lit(1.0);
    let latest_date = market_lf
        .clone()
        .select([col("Date").max()])
        .collect()?
        .column("Date")?
        .get(0)?
        .to_string()
        .replace('"', "");
    let latest_prices_df = market_lf
        .sort(["Code", "Date"], SortMultipleOptions::default())
        .with_column(
            when(daily_return.clone().gt_eq(lit(0.0)))
                .then(daily_return.clone())
                .otherwise(-daily_return)
                .over([col("Code")])
                .alias("daily_abs_return"),
        )
        .with_column(
            col("daily_abs_return")
                .rolling_mean(RollingOptionsFixedWindow {
                    window_size: atr_lookback_days,
                    min_periods: 2,
                    ..Default::default()
                })
                .over([col("Code")])
                .alias("atr_percent"),
        )
        // 銘柄間で最新日が完全には揃わないことがあるため、全体の最新日で絞り込まない。
        // 各銘柄の最終行を採用し、部分同期されたParquetでも保有銘柄の評価価格を更新する。
        .group_by([col("Code")])
        .agg([
            col("Date").last().alias("price_date"),
            col("AdjC").last().alias("AdjC"),
            col("atr_percent").last().alias("atr_percent"),
        ])
        .select([
            col("Code"),
            col("price_date"),
            col("AdjC"),
            col("atr_percent"),
        ])
        .collect()?;

    let codes = latest_prices_df.column("Code")?.str()?;
    let price_date_values = latest_prices_df.column("price_date")?.str()?;
    let closes = latest_prices_df.column("AdjC")?.f64()?;
    let atr_values = latest_prices_df.column("atr_percent")?.f64()?;
    let mut latest_prices = HashMap::new();
    let mut price_dates = HashMap::new();
    let mut atr_percent = HashMap::new();
    for (((code, price_date), close), atr) in codes
        .into_iter()
        .zip(price_date_values.into_iter())
        .zip(closes.into_iter())
        .zip(atr_values.into_iter())
    {
        if let (Some(code), Some(price_date), Some(close)) = (code, price_date, close) {
            latest_prices.insert(code.to_string(), close);
            price_dates.insert(code.to_string(), price_date.to_string());
            if let Some(atr) = atr.filter(|value| value.is_finite() && *value > 0.0) {
                atr_percent.insert(code.to_string(), atr);
            }
        }
    }
    Ok(MarketPriceSnapshot {
        date: latest_date,
        prices: latest_prices,
        price_dates,
        atr_percent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractional_qty_respects_position_and_total_budget() {
        let conn = Connection::open_in_memory().unwrap();
        init_db_extended(&conn).unwrap();

        assert_eq!(
            calculate_fractional_buy_qty(&conn, 4_000, 3_000, 1_000.0).unwrap(),
            Some((3, 3_000.0))
        );
        record_virtual_buy(&conn, "0001", "テスト銘柄", 1_000.0, 3).unwrap();
        let status: String = conn
            .query_row("SELECT status FROM active_positions WHERE code = '0001'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(status, "HOLDING");

        assert_eq!(
            calculate_fractional_buy_qty(&conn, 4_000, 3_000, 1_000.0).unwrap(),
            Some((1, 1_000.0))
        );
        record_virtual_buy(&conn, "0002", "テスト銘柄2", 1_000.0, 1).unwrap();

        assert_eq!(
            calculate_fractional_buy_qty(&conn, 4_000, 3_000, 1_000.0).unwrap(),
            None
        );
    }

    #[test]
    fn legacy_pending_buys_are_activated_once() {
        let conn = Connection::open_in_memory().unwrap();
        init_db_extended(&conn).unwrap();
        conn.execute(
            "INSERT INTO active_positions (code, name, entry_date, entry_price, qty, highest_price, current_price, status, holding_days) VALUES ('0001', 'テスト銘柄', '2026-01-01', 100.0, 1, 100.0, 100.0, 'PENDING_BUY', 0)",
            [],
        )
        .unwrap();

        assert_eq!(activate_legacy_pending_buys(&conn).unwrap(), 1);
        assert_eq!(activate_legacy_pending_buys(&conn).unwrap(), 0);
        let status: String = conn
            .query_row("SELECT status FROM active_positions WHERE code = '0001'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(status, "HOLDING");
    }

    #[test]
    fn snapshot_normalizes_jquants_and_yahoo_codes_before_selecting_latest_price() {
        let path = std::env::temp_dir().join(format!(
            "jp_stock_snapshot_{}_{}.parquet",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut df = df!(
            "Date" => ["2026-09-17", "2026-09-30", "2026-09-17", "2026-09-30"],
            "Code" => ["67680", "6768", "85180", "8518"],
            "AdjC" => [778.0, 810.0, 156.0, 170.0]
        )
        .unwrap();
        ParquetWriter::new(std::fs::File::create(&path).unwrap())
            .finish(&mut df)
            .unwrap();

        let snapshot = market_price_snapshot_from_parquet(path.to_str().unwrap(), 2).unwrap();
        assert_eq!(snapshot.date, "2026-09-30");
        assert_eq!(snapshot.prices.get("6768"), Some(&810.0));
        assert_eq!(snapshot.prices.get("8518"), Some(&170.0));
        assert_eq!(snapshot.price_dates.get("6768").map(String::as_str), Some("2026-09-30"));
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn latest_prices_update_valuation_and_high_water_mark() {
        let conn = Connection::open_in_memory().unwrap();
        init_db_extended(&conn).unwrap();
        conn.execute(
            "INSERT INTO active_positions (code, name, entry_date, entry_price, qty, highest_price, current_price, status, holding_days) VALUES ('0001', 'テスト銘柄', '2026-01-01', 100.0, 10, 100.0, 100.0, 'HOLDING', 0)",
            [],
        )
        .unwrap();

        let latest_prices = HashMap::from([(String::from("0001"), 110.0)]);
        evaluate_and_exit_positions_with_prices(&conn, &latest_prices, "2026-01-02")
            .await
            .unwrap();

        let (current_price, highest_price, holding_days): (f64, f64, i64) = conn
            .query_row(
                "SELECT current_price, highest_price, holding_days FROM active_positions WHERE code = '0001'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!((current_price, highest_price, holding_days), (110.0, 110.0, 1));
    }

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
    let mut stmt = conn.prepare("SELECT code FROM active_positions WHERE status = 'HOLDING'")?;
    let codes: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);

    let mut latest_prices = HashMap::new();
    for code in codes {
        let latest_close: Option<f64> = conn
            .query_row(
                "SELECT close FROM OHLC WHERE code = ?1 ORDER BY date DESC LIMIT 1",
                [code.clone()],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(price) = latest_close {
            latest_prices.insert(code, price);
        }
    }

    let evaluation_date = Local::now().format("%Y-%m-%d").to_string();
    evaluate_and_exit_positions_with_prices(conn, &latest_prices, &evaluation_date).await
}

/// Parquet など外部の最新終値を使って、保有ポジションを評価する。
/// `latest_prices` に価格がない銘柄は、古い価格のまま誤判定しないよう評価を行わない。
pub async fn evaluate_and_exit_positions_with_prices(
    conn: &Connection,
    latest_prices: &HashMap<String, f64>,
    evaluation_date: &str,
) -> rusqlite::Result<()> {
    let default_strategy = ExitStrategySettings {
        atr_lookback_days: 14,
        stop_loss_atr_multiplier: 1.5,
        take_profit_atr_multiplier: 2.5,
        trailing_stop_atr_multiplier: 1.5,
        max_stop_loss_percent: 15.0,
        fallback_atr_percent: 5.0,
        overrides: HashMap::new(),
    };
    evaluate_and_exit_positions_with_strategy(
        conn,
        latest_prices,
        evaluation_date,
        &HashMap::new(),
        &default_strategy,
    )
    .await
}

/// 最新価格と銘柄別ATR相当値を使って、設定済みの出口戦略を評価する。
pub async fn evaluate_and_exit_positions_with_strategy(
    conn: &Connection,
    latest_prices: &HashMap<String, f64>,
    evaluation_date: &str,
    atr_percent: &HashMap<String, f64>,
    exit_strategy: &ExitStrategySettings,
) -> rusqlite::Result<()> {

    // HOLDING 中のポジションを全件取得して評価する。
    // 保有日数は、最新価格を取得できた銘柄だけ加算する。
    let mut stmt = conn.prepare(
        "SELECT code, entry_date, entry_price, qty, highest_price, holding_days, last_evaluated_date FROM active_positions WHERE status = 'HOLDING'"
    )?;
    let mut rows = stmt.query([])?;

    let mut pending_exits = Vec::new();

    while let Some(row) = rows.next()? {
        let code: String = row.get(0)?;
        let _entry_date: String = row.get(1)?;
        let entry_price: f64 = row.get(2)?;
        let _qty: i64 = row.get(3)?;
        let mut highest_price: f64 = row.get(4)?;
        let previous_holding_days = row.get::<_, i64>(5)?;
        let last_evaluated_date: Option<String> = row.get(6)?;

        if let Some(&current_price) = latest_prices.get(&code) {
            if !current_price.is_finite() || current_price <= 0.0 {
                continue;
            }

            // 同一の市場データを手動実行しても、保有日数を二重加算しない。
            if last_evaluated_date.as_deref() == Some(evaluation_date) {
                continue;
            }
            let holding_days = previous_holding_days + 1;

            let symbol_override = exit_strategy.overrides.get(&code);
            let atr = atr_percent
                .get(&code)
                .copied()
                .unwrap_or(exit_strategy.fallback_atr_percent / 100.0);
            let stop_loss_pct = symbol_override
                .and_then(|rule| rule.stop_loss_percent)
                .map(|value| value / 100.0)
                .unwrap_or(atr * exit_strategy.stop_loss_atr_multiplier)
                .min(exit_strategy.max_stop_loss_percent / 100.0);
            let take_profit_pct = symbol_override
                .and_then(|rule| rule.take_profit_percent)
                .map(|value| value / 100.0)
                .unwrap_or(atr * exit_strategy.take_profit_atr_multiplier);
            let trailing_stop_pct = symbol_override
                .and_then(|rule| rule.trailing_stop_percent)
                .map(|value| value / 100.0)
                .unwrap_or(atr * exit_strategy.trailing_stop_atr_multiplier);

            // ① 最高値の更新チェック
            if current_price > highest_price {
                highest_price = current_price;
            }

            // 決済の有無にかかわらず、レポート用の現在値・最高値・保有日数を先に保存する。
            conn.execute(
                "UPDATE active_positions SET current_price = ?1, highest_price = ?2, holding_days = ?3, last_evaluated_date = ?4 WHERE code = ?5",
                params![current_price, highest_price, holding_days, evaluation_date, code],
            )?;

            // ② 各種損益率の計算
            let current_pl_pct = (current_price - entry_price) / entry_price; // 購入原価からの損益率
            let max_gain_pct = (highest_price - entry_price) / entry_price;   // これまでの最大利益率
            let drop_from_peak_pct = (highest_price - current_price) / highest_price; // 最高値からの下落率

            let mut is_exit = false;
            let mut exit_reason = String::new();

            // ③ 決済判定ロジック
            if current_pl_pct <= -stop_loss_pct {
                // ① ATR連動損切りライン（最大損失の安全上限付き）に接触
                is_exit = true;
                exit_reason = format!("ATR損切り(-{:.1}%)", stop_loss_pct * 100.0);
            } else if max_gain_pct >= take_profit_pct && drop_from_peak_pct >= trailing_stop_pct {
                // ② ATR連動トレーリング利確（利確目標到達後、最高値から指定幅下落）
                is_exit = true;
                exit_reason = format!("ATRトレーリング利確(ピークから-{:.1}%)", trailing_stop_pct * 100.0);
            } else if holding_days >= 10 {
                // ③ 10日タイムアウト制限
                is_exit = true;
                exit_reason = "10日タイムアウト制限".to_string();
            }

            // ④ 決済判定に該当した場合は PENDING_SELL に変更（明朝始値で約定）
            if is_exit {
                pending_exits.push((code.clone(), exit_reason));
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
