fn nonlinear(x: f64) -> f64 {
    (2.0 * x).tanh()
}

pub fn compute_score(feature: &Vec<f64>) -> f64 {

    // =========================
    // 特徴量の展開
    // =========================
    let mom1 = feature[0];
    let mom5 = feature[1];
    let vol20 = feature[3];
    let interaction = feature[4];

    // =========================
    // 線形結合
    // =========================
    let raw_score =
    0.3 * mom1 +
    0.3 * mom5 +
    0.2 * interaction -
    0.1 * vol20;

    // =========================
    // 非線形変換（return）
    // =========================
    nonlinear(raw_score)
}