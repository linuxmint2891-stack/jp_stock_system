pub fn compute_features(returns: &Vec<f64>) -> Vec<Vec<f64>> {

    let mut features = Vec::new();

    for i in 0..returns.len() {

        let mom1 = returns[i];

        let mom5 = mom1;   // 仮（あとでちゃんと作る）
        let mom20 = mom1;
        let vol20 = returns[i].abs();

        let interaction = mom5 * (1.0 - vol20);

        features.push(vec![
            mom1,
            mom5,
            mom20,
            vol20,
            interaction,
        ]);
    }

    features
}