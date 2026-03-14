mod api;
mod db;
mod model;

use reqwest::Client;
use rusqlite::Connection;
use chrono::Local;
use futures::future::join_all;

use api::fetch_stock;
use db::{init_db,insert_stock};

#[tokio::main]
async fn main()->Result<(),Box<dyn std::error::Error>>{

    let conn = Connection::open("stocks.db")?;
    init_db(&conn);

    let csv = std::fs::read_to_string("data/jpx_codes.csv")?;

    let codes:Vec<String> = csv
        .lines()
        .map(|c| format!("{}.T",c))
        .collect();

    let client = Client::new();

    let tasks:Vec<_> = codes
        .iter()
        .map(|c| fetch_stock(&client,c))
        .collect();

    let results: Vec<Option<model::Meta>> = join_all(tasks).await;

    let today = Local::now().format("%Y-%m-%d").to_string();

    for stock in results.into_iter().flatten(){

        println!(
            "{} {} {}",
            stock.symbol,
            stock.short_name,
            stock.regular_market_price
        );

        insert_stock(&conn,&today,&stock);
    }

    println!("全銘柄保存完了");

    Ok(())
}