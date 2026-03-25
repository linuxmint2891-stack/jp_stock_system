pub fn select_top_bottom_k(scores: &[f64], k: usize) -> (Vec<usize>, Vec<usize>) {
    let n = scores.len();

    // 🔥 安全制御
    let k = k.min(n / 2);

    // インデックス生成（絶対安全）
    let mut idx: Vec<usize> = (0..n).collect();

    // スコアでソート（降順）
    idx.sort_by(|&i, &j| scores[j].partial_cmp(&scores[i]).unwrap());

    // 上位k
    let long: Vec<usize> = idx.iter().take(k).cloned().collect();

    // 下位k（逆順から）
    let short: Vec<usize> = idx.iter().rev().take(k).cloned().collect();

    // 🔍 デバッグ（重要）
    for &i in &long {
        assert!(i < n, "long index overflow: {}", i);
    }
    for &i in &short {
        assert!(i < n, "short index overflow: {}", i);
    }

    (long, short)
}