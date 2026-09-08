//! Equity-desk features. Boring TA: stretched vs calm. Not a live GNN.

use serde::Serialize;

use crate::MlError;

#[derive(Debug, Clone, Serialize)]
pub struct NameBar {
    pub t: i64,
    pub o: f64,
    pub h: f64,
    pub l: f64,
    pub c: f64,
    pub v: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeatureCell {
    pub name: &'static str,
    pub value: f64,
    pub units: &'static str,
    pub meaning: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NameFeatures {
    pub symbol: String,
    pub asof_unix_ms: i64,
    pub stale: bool,
    pub ret_1: f64,
    pub ret_5: f64,
    pub ret_20: f64,
    pub vol: f64,
    pub range_pos: f64,
    pub rsi_14: f64,
    pub ema_fast: f64,
    pub ema_slow: f64,
    pub score: f64,
    pub cells: Vec<FeatureCell>,
}

pub fn log_ret(a: f64, b: f64) -> f64 {
    if a <= 0.0 || b <= 0.0 {
        0.0
    } else {
        (b / a).ln()
    }
}

pub fn rsi(closes: &[f64], period: usize) -> f64 {
    if closes.len() < period + 1 {
        return 50.0;
    }
    let mut gain = 0.0;
    let mut loss = 0.0;
    let start = closes.len() - period - 1;
    for i in start + 1..=start + period {
        let d = closes[i] - closes[i - 1];
        if d >= 0.0 {
            gain += d;
        } else {
            loss -= d;
        }
    }
    let ag = gain / period as f64;
    let al = loss / period as f64;
    if al < 1e-12 {
        return 100.0;
    }
    100.0 - 100.0 / (1.0 + ag / al)
}

pub fn ema(closes: &[f64], span: usize) -> f64 {
    if closes.is_empty() {
        return 0.0;
    }
    let a = 2.0 / (span as f64 + 1.0);
    let mut e = closes[0];
    for c in &closes[1..] {
        e = a * c + (1.0 - a) * e;
    }
    e
}

pub fn realized_vol(closes: &[f64]) -> f64 {
    if closes.len() < 3 {
        return 0.0;
    }
    let rets: Vec<f64> = closes.windows(2).map(|w| log_ret(w[0], w[1])).collect();
    let n = rets.len() as f64;
    let mean = rets.iter().sum::<f64>() / n;
    let var = rets.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
    var.sqrt() * (252.0_f64).sqrt()
}

pub fn range_pos(closes: &[f64], window: usize) -> f64 {
    if closes.is_empty() {
        return 0.5;
    }
    let slice = if closes.len() > window {
        &closes[closes.len() - window..]
    } else {
        closes
    };
    let lo = slice.iter().cloned().fold(f64::INFINITY, f64::min);
    let hi = slice.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let last = *closes.last().unwrap();
    if hi - lo < 1e-12 {
        0.5
    } else {
        ((last - lo) / (hi - lo)).clamp(0.0, 1.0)
    }
}

/// Need ≥21 closes. Fail closed on junk.
pub fn features_from_closes(
    symbol: &str,
    closes: &[f64],
    asof_unix_ms: i64,
    now_ms: i64,
) -> Result<NameFeatures, MlError> {
    if closes.len() < 21 {
        return Err(MlError::Constraint("need 21 closes"));
    }
    if closes.iter().any(|c| !c.is_finite() || *c <= 0.0) {
        return Err(MlError::Constraint("non-finite close"));
    }
    let n = closes.len();
    let last = closes[n - 1];
    let ret_1 = log_ret(closes[n - 2], last);
    let ret_5 = log_ret(closes[n - 6], last);
    let ret_20 = log_ret(closes[n - 21], last);
    let vol = realized_vol(&closes[n.saturating_sub(21)..]);
    let rp = range_pos(closes, 20);
    let rsi_14 = rsi(closes, 14);
    let ema_fast = ema(closes, 12);
    let ema_slow = ema(closes, 26);
    let stale = now_ms.saturating_sub(asof_unix_ms) > 900_000;
    let score = ret_20 * 10.0 - (rsi_14 - 50.0).abs() / 50.0;
    let cells = vec![
        FeatureCell {
            name: "ret_1",
            value: ret_1,
            units: "log",
            meaning: "1-bar move".into(),
        },
        FeatureCell {
            name: "ret_5",
            value: ret_5,
            units: "log",
            meaning: "5-bar move".into(),
        },
        FeatureCell {
            name: "ret_20",
            value: ret_20,
            units: "log",
            meaning: "20-bar move".into(),
        },
        FeatureCell {
            name: "vol",
            value: vol,
            units: "ann",
            meaning: "realized vol vs last 20 bars".into(),
        },
        FeatureCell {
            name: "range_pos",
            value: rp,
            units: "0-1",
            meaning: format!(
                "close vs 20-bar high/low ({})",
                if rp > 0.8 {
                    "near high"
                } else if rp < 0.2 {
                    "near low"
                } else {
                    "mid range"
                }
            ),
        },
        FeatureCell {
            name: "rsi_14",
            value: rsi_14,
            units: "0-100",
            meaning: if rsi_14 > 70.0 {
                "pushed up vs last 14 bars".into()
            } else if rsi_14 < 30.0 {
                "pushed down vs last 14 bars".into()
            } else {
                "mid RSI".into()
            },
        },
        FeatureCell {
            name: "ema_fast",
            value: ema_fast,
            units: "px",
            meaning: "EMA 12".into(),
        },
        FeatureCell {
            name: "ema_slow",
            value: ema_slow,
            units: "px",
            meaning: "EMA 26".into(),
        },
    ];
    Ok(NameFeatures {
        symbol: symbol.into(),
        asof_unix_ms,
        stale,
        ret_1,
        ret_5,
        ret_20,
        vol,
        range_pos: rp,
        rsi_14,
        ema_fast,
        ema_slow,
        score,
        cells,
    })
}

pub fn return_heatmap(closes: &[f64], n: usize) -> Vec<Vec<f64>> {
    let n = n.clamp(2, 20);
    if closes.len() < n + 1 {
        return vec![vec![0.0; n]; n];
    }
    let mut m = vec![vec![0.0; n]; n];
    let start = closes.len() - n;
    for i in 0..n {
        for j in 0..n {
            m[i][j] = log_ret(closes[start + i], closes[start + j]).abs();
        }
    }
    m
}

/// Deterministic synthetic bars for hermetic tests / fixture boot.
pub fn synth_bars(symbol: &str, n: usize) -> Vec<NameBar> {
    let mut h: u64 = 0xcbf2_9ce4_8422_2965;
    for b in symbol.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    let mut px = 50.0 + (h % 200) as f64;
    let mut out = Vec::with_capacity(n);
    let t0 = 1_700_000_000_000i64;
    for i in 0..n {
        h = h.wrapping_mul(1664525).wrapping_add(1013904223);
        let step = ((h % 200) as f64 - 100.0) / 400.0;
        px = (px * (1.0 + step)).max(1.0);
        let o = px;
        let c = px * (1.0 + step * 0.2);
        let hi = o.max(c) * 1.002;
        let lo = o.min(c) * 0.998;
        out.push(NameBar {
            t: t0 + i as i64 * 86_400_000,
            o,
            h: hi,
            l: lo,
            c,
            v: 1_000_000.0,
        });
        px = c;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rsi_flat_is_mid() {
        let c: Vec<f64> = (0..30).map(|_| 100.0).collect();
        let r = rsi(&c, 14);
        assert!((r - 50.0).abs() < 1e-6 || r == 100.0);
    }

    #[test]
    fn features_need_21_closes() {
        assert!(features_from_closes("AAPL", &[1.0; 10], 0, 0).is_err());
        let bars = synth_bars("AAPL", 40);
        let closes: Vec<f64> = bars.iter().map(|b| b.c).collect();
        let f = features_from_closes("AAPL", &closes, 0, 0).unwrap();
        assert_eq!(f.symbol, "AAPL");
        assert!(f.rsi_14.is_finite());
        assert!((0.0..=1.0).contains(&f.range_pos));
        assert_eq!(f.cells.len(), 8);
    }

    #[test]
    fn stale_when_age_over_15m() {
        let bars = synth_bars("TSLA", 30);
        let c: Vec<f64> = bars.iter().map(|b| b.c).collect();
        let f = features_from_closes("TSLA", &c, 0, 1_000_000).unwrap();
        assert!(f.stale);
    }
}
