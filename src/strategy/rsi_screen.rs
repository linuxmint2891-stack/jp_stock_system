use rusqlite::Connection;

use crate::indicator::rsi::rsi;

pub fn rsi_screen(conn:&Connection){

    let mut stmt = conn.prepare(
        "
        SELECT DISTINCT code
        FROM ohlc
        "
    ).unwrap();

    let codes = stmt.query_map([], |row| {
        row.get::<_,String>(0)
    }).unwrap();

    let mut results:Vec<(String,f64)> = Vec::new();

    for code in codes{

        let code = code.unwrap();

        let mut stmt = conn.prepare(
            "
            SELECT close
            FROM ohlc
            WHERE code=?1
            ORDER BY date
            "
        ).unwrap();

        let prices:Vec<f64> = stmt
            .query_map([&code], |row| row.get(0))
            .unwrap()
            .map(|x|x.unwrap())
            .collect();

        if prices.len() < 20{
            continue;
        }

        let rsi_values = rsi(&prices,14);

        if let Some(Some(v)) = rsi_values.last(){

            results.push((code,*v));

        }
    }

    results.sort_by(|a,b| a.1.partial_cmp(&b.1).unwrap());

    println!("\nRSIランキング (低い順)");

    for (i,(code,v)) in results.iter().take(20).enumerate(){

        println!(
            "{:02} {} RSI {:.2}",
            i+1,
            code,
            v
        );
    }
}