pub fn softmax_weights(sharpes: &Vec<f64>, beta: f64) -> Vec<f64> {
    let exp_vals: Vec<f64> = sharpes
        .iter()
        .map(|s| (beta * s).exp())
        .collect();

    let sum: f64 = exp_vals.iter().sum();

    exp_vals.iter().map(|v| v / sum).collect()
}