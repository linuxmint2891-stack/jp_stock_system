use reqwest;

pub async fn load_codes() -> Vec<String> {
    let url = "https://www.jpx.co.jp/markets/statistics-equities/misc/tvdivq0000001vg2-att/data_j.csv";

    let resp = reqwest::get(url).await.unwrap();
    let text = resp.text().await.unwrap();

    let mut codes = Vec::new();

    for line in text.lines() {
        let cols: Vec<&str> = line.split(',').collect();

        if cols.len() > 1 {
            let code = cols[1].trim();

            if code.len() == 4 && code.chars().all(|c: char| c.is_ascii_digit()) {
                codes.push(code.to_string());
            }
        }
    }

    println!("銘柄数: {}", codes.len());

    codes
}