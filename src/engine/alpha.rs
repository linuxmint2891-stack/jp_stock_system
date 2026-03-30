pub fn compute_score(f: &Vec<f64>) -> f64 {
    -f[1] + 0.3 * (-f[2])
}