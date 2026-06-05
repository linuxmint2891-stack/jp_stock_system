pub fn rank(vec: &Vec<f64>) -> Vec<f64> {
    let mut pairs: Vec<(usize, f64)> =
        vec.iter().cloned().enumerate().collect();

    pairs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

    let mut ranks = vec![0.0; vec.len()];

    for (i, (idx, _)) in pairs.iter().enumerate() {
        ranks[*idx] = i as f64;
    }

    ranks
}