use crate::engine::portfolio::select_top_bottom_k;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BacktestError {
    InsufficientPeriods,
    LengthMismatch,
    InvalidPortfolioSize,
    InsufficientCandidates { period: usize, candidates: usize, required: usize },
    NonFiniteValue,
}

/// スコア時点 t の銘柄を、リターン時点 t + 1 の収益率で評価する。
/// 空系列・日付ずれ・候補数不足は、見かけ上もっともらしい結果を返さずエラーにする。
pub fn run_backtest(
    scores_series: &[Vec<f64>],
    returns_series: &[Vec<f64>],
    k: usize,
) -> Result<Vec<f64>, BacktestError> {
    if scores_series.len() < 2 || returns_series.len() < 2 {
        return Err(BacktestError::InsufficientPeriods);
    }
    if scores_series.len() != returns_series.len() {
        return Err(BacktestError::LengthMismatch);
    }
    if k == 0 {
        return Err(BacktestError::InvalidPortfolioSize);
    }

    for (period, (scores, returns)) in scores_series.iter().zip(returns_series).enumerate() {
        if scores.len() != returns.len() {
            return Err(BacktestError::LengthMismatch);
        }
        let required = k.saturating_mul(2);
        if scores.len() < required {
            return Err(BacktestError::InsufficientCandidates {
                period,
                candidates: scores.len(),
                required,
            });
        }
        if scores.iter().chain(returns).any(|value| !value.is_finite()) {
            return Err(BacktestError::NonFiniteValue);
        }
    }

    let mut pnl_series = Vec::with_capacity(scores_series.len() - 1);
    for t in 0..scores_series.len() - 1 {
        let (long_idx, short_idx) = select_top_bottom_k(&scores_series[t], k);
        let returns_next = &returns_series[t + 1];

        let long_pnl: f64 = long_idx.iter().map(|&i| returns_next[i]).sum();
        let short_pnl: f64 = short_idx.iter().map(|&i| -returns_next[i]).sum();
        pnl_series.push((long_pnl + short_pnl) / (2 * k) as f64);
    }

    Ok(pnl_series)
}

/// 空の損益系列ではシャープレシオを定義しない。
pub fn sharpe(pnl: &[f64]) -> Option<f64> {
    if pnl.is_empty() || pnl.iter().any(|value| !value.is_finite()) {
        return None;
    }

    let mean = pnl.iter().sum::<f64>() / pnl.len() as f64;
    let var = pnl.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / pnl.len() as f64;
    Some(mean / (var.sqrt() + 1e-8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_or_insufficient_series() {
        assert_eq!(run_backtest(&[], &[], 1), Err(BacktestError::InsufficientPeriods));
        assert_eq!(
            run_backtest(&[vec![1.0], vec![2.0]], &[vec![0.0], vec![0.1]], 1),
            Err(BacktestError::InsufficientCandidates { period: 0, candidates: 1, required: 2 })
        );
    }

    #[test]
    fn calculates_a_valid_long_short_return() {
        let pnl = run_backtest(
            &[vec![2.0, 1.0], vec![2.0, 1.0]],
            &[vec![0.0, 0.0], vec![0.10, -0.10]],
            1,
        )
        .unwrap();
        assert_eq!(pnl, vec![0.10]);
    }

    #[test]
    fn sharpe_is_none_for_empty_input() {
        assert_eq!(sharpe(&[]), None);
    }
}
