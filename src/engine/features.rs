pub fn build_features(returns: &[f64]) -> Vec<f64> {
    if returns.len() < 10 {
        return vec![0.0; 5];
    }

    let r1 = returns[returns.len()-1];
    let r3 = returns.iter().rev().take(3).sum::<f64>();
    let r5 = returns.iter().rev().take(5).sum::<f64>();
    let r10 = returns.iter().rev().take(10).sum::<f64>();

    let vol = std_dev(&returns[returns.len()-10..]);

    vec![r1, r3, r5, r10, vol]
}
fn std_dev(x: &[f64]) -> f64 {
    let mean = x.iter().sum::<f64>() / x.len() as f64;

    let var = x.iter()
        .map(|v| (v - mean).powi(2))
        .sum::<f64>() / x.len() as f64;

    var.sqrt()
}