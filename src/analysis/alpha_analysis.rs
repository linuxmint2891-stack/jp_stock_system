pub fn sharpe(returns: &Vec<f64>) -> f64 {
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;

    let var = returns.iter()
        .map(|r| (r - mean).powi(2))
        .sum::<f64>() / returns.len() as f64;

    if var == 0.0 {
        return 0.0;
    }

    mean / var.sqrt()
}

pub fn sharpe_split(returns: &Vec<f64>) -> (f64, f64) {
    let mid = returns.len() / 2;

    let first = sharpe(&returns[..mid].to_vec());
    let second = sharpe(&returns[mid..].to_vec());

    (first, second)
}

pub fn correlation(x: &Vec<f64>, y: &Vec<f64>) -> f64 {
    let mean_x = x.iter().sum::<f64>() / x.len() as f64;
    let mean_y = y.iter().sum::<f64>() / y.len() as f64;

    let mut num = 0.0;
    let mut denom_x = 0.0;
    let mut denom_y = 0.0;

    for i in 0..x.len() {
        let dx = x[i] - mean_x;
        let dy = y[i] - mean_y;

        num += dx * dy;
        denom_x += dx * dx;
        denom_y += dy * dy;
    }

    if denom_x == 0.0 || denom_y == 0.0 {
        return 0.0;
    }

    num / (denom_x.sqrt() * denom_y.sqrt())
}