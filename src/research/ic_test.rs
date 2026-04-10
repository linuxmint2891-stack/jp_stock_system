pub fn run() {
    let n_assets = 50;
    let n_time = 200;

    // ===============================
    // prices
    // ===============================
    let mut prices = vec![vec![0.0; n_time]; n_assets];

    use rand::Rng;
let mut rng = rand::thread_rng();

pub fn compute_ic(feature: &Vec<f64>, future_returns: &Vec<f64>) -> f64 {
    let n = feature.len();

    let mean_x = feature.iter().sum::<f64>() / n as f64;
    let mean_y = future_returns.iter().sum::<f64>() / n as f64;

    let mut num = 0.0;
    let mut denom_x = 0.0;
    let mut denom_y = 0.0;

    for i in 0..n {
        let dx = feature[i] - mean_x;
        let dy = future_returns[i] - mean_y;

        num += dx * dy;
        denom_x += dx * dx;
        denom_y += dy * dy;
    }

    num / (denom_x.sqrt() * denom_y.sqrt())
}

for i in 0..n_assets {
    for t in 0..n_time {
        prices[i][t] =
            100.0
            + (i as f64) * 0.5
            + (t as f64 * 0.1).sin() * 5.0
            + rng.gen_range(-1.0..1.0); // ← ここ追加
    }
}

    // ===============================
    // returns
    // ===============================
    let mut returns = vec![vec![0.0; n_time - 1]; n_assets];

    for i in 0..n_assets {
        for t in 1..n_time {
            returns[i][t - 1] =
                (prices[i][t] - prices[i][t - 1]) / prices[i][t - 1];
        }
    }

    // ===============================
    // momentum
    // ===============================
    let mut momentum = vec![vec![0.0; n_time - 6]; n_assets];

    for i in 0..n_assets {
        for t in 5..(n_time - 1) {
            momentum[i][t - 5] =
                (prices[i][t] - prices[i][t - 5]) / prices[i][t - 5];
        }
    }

    // ===============================
    // rank function（←ここに追加）
    // ===============================
    fn rank(x: &Vec<f64>) -> Vec<f64> {
        let mut pairs: Vec<(usize, f64)> =
            x.iter().cloned().enumerate().collect();

        pairs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        let mut r = vec![0.0; x.len()];
        for (i, (idx, _)) in pairs.iter().enumerate() {
            r[*idx] = i as f64 / x.len() as f64;
        }

        r
    }

    // ===============================
    // correlation function（←ここに追加）
    // ===============================
    fn correlation(x: &Vec<f64>, y: &Vec<f64>) -> f64 {
        let n = x.len() as f64;

        let mean_x = x.iter().sum::<f64>() / n;
        let mean_y = y.iter().sum::<f64>() / n;

        let mut num = 0.0;
        let mut den_x = 0.0;
        let mut den_y = 0.0;

        for i in 0..x.len() {
            let dx = x[i] - mean_x;
            let dy = y[i] - mean_y;
            num += dx * dy;
            den_x += dx * dx;
            den_y += dy * dy;
        }

        if den_x == 0.0 || den_y == 0.0 {
            return 0.0;
        }

        num / (den_x.sqrt() * den_y.sqrt())
    }

    // ===============================
    // IC計測（←ここが本体）
    // ===============================
    let mut ic_series = vec![];

    for t in 0..(n_time - 6) {

        let mom_t: Vec<f64> =
            (0..n_assets).map(|i| momentum[i][t]).collect();

        let future_ret: Vec<f64> =
            (0..n_assets).map(|i| returns[i][t + 5]).collect();

        // 🔥 cross-sectional rank
        let rank_mom = rank(&mom_t);

        // 🔥 alpha（まずはこれ固定）
        let alpha: Vec<f64> = rank_mom.iter().map(|v| -v).collect();

        // 🔥 IC
        let ic = correlation(&alpha, &future_ret);
        ic_series.push(ic);
    }

    // ===============================
    // 結果出力（←ここを見る）
    // ===============================
    let ic_mean =
        ic_series.iter().sum::<f64>() / ic_series.len() as f64;

    let ic_std = (
        ic_series.iter()
            .map(|v| (v - ic_mean).powi(2))
            .sum::<f64>() / ic_series.len() as f64
    ).sqrt();

    println!("IC mean: {}", ic_mean);
    println!("IC std: {}", ic_std);
}