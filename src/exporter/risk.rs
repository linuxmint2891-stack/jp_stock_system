pub fn covariance(returns: &Vec<Vec<f64>>) -> Vec<Vec<f64>> {
    let n = returns.len();
    let t = returns[0].len();

    let mut cov = vec![vec![0.0; n]; n];

    for i in 0..n {
        for j in 0..n {
            let mut s = 0.0;
            for k in 0..t {
                s += returns[i][k] * returns[j][k];
            }
            cov[i][j] = s / t as f64;
        }
    }

    cov
}
pub fn vol_scale(weights: &mut Vec<f64>, vol: &Vec<f64>) {
    for i in 0..weights.len() {
        if vol[i] > 0.0 {
            weights[i] /= vol[i];
        }
    }
}
