pub fn compute_features(returns: &Vec<f64>) -> Vec<Vec<f64>> {
    let n = returns.len();

    let mean = returns.iter().sum::<f64>() / n as f64;

    let mut features = Vec::with_capacity(n);

    for i in 0..n {
        let r = returns[i];

        // クロスセクション
        let cs = r - mean;

        // 👇 追加：過去平均との差（簡易リバーサル）
        let prev = if i > 0 { returns[i - 1] } else { 0.0 };

        features.push(vec![
            r,        // f[0]
            cs,       // f[1]
            r - prev, // f[2] ← NEW（時間構造）
        ]);
    }

    features
}