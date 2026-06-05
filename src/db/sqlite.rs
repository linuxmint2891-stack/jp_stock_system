use rusqlite::{Connection, OptionalExtension};

pub fn get_latest_date(
    conn:&Connection,
    code:&str
)->Option<i64>{

    let result: Option<Option<i64>> = conn.query_row(

        "
        SELECT MAX(date)
        FROM OHLC
        WHERE code=?1
        ",

        [code],

        |row| row.get(0)

    ).optional().unwrap();

    result.flatten()

}

pub fn init_db(conn:&Connection){

    conn.execute(
        "
        CREATE TABLE IF NOT EXISTS OHLC(

            code TEXT,
            date INTEGER,
            open REAL,
            high REAL,
            low REAL,
            close REAL,
            volume INTEGER,

            PRIMARY KEY(code,date)

        )
        ",
        []
    ).unwrap();

}

use rusqlite::params;

pub fn insert_OHLC(
    conn:&Connection,
    code:&str,
    row:&crate::model::ohlc::OHLC
){

    conn.execute(
        "
        INSERT OR IGNORE INTO OHLC
        (code,date,open,high,low,close,volume)

        VALUES (?1,?2,?3,?4,?5,?6,?7)
        ",
        params![
            code,
            row.timestamp,
            row.open,
            row.high,
            row.low,
            row.close,
            row.volume
            ]
    ).unwrap();

}