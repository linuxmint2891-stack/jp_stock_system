// src/portfolio/combine.rs

pub fn softmax(scores: &[f64]) -> Vec<f64> {
    let max = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let exp_vals: Vec<f64> = scores.iter()
        .map(|s| (s - max).exp())
        .collect();

    let sum: f64 = exp_vals.iter().sum();

    exp_vals.iter().map(|v| v / sum).collect()
}