use rusqlite::{Connection,params};
use crate::model::Meta;

pub fn init_db(conn:&Connection){

    conn.execute(
        "CREATE TABLE IF NOT EXISTS stocks(
        id INTEGER PRIMARY KEY,
        date TEXT,
        code TEXT,
        name TEXT,
        price REAL,
        volume INTEGER
        )",
        [],
    ).unwrap();
}

pub fn insert_stock(
    conn:&Connection,
    date:&str,
    stock:&Meta
){

    conn.execute(
        "INSERT INTO stocks(date,code,name,price,volume)
        VALUES(?1,?2,?3,?4,?5)",
        params![
            date,
            stock.symbol,
            stock.short_name,
            stock.regular_market_price,
            stock.regular_market_volume
        ],
    ).unwrap();
}