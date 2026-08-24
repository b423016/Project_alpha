//! European Black–Scholes. American early exercise is out of v1 (DTE<7 skipped).

use neural_router_domain::Greeks;

use crate::MlError;

const SQRT_2PI: f64 = 2.506_628_238_584_699;
const INV_SQRT2: f64 = 0.707_106_781_186_547_6;

fn erf(x: f64) -> f64 {
    // Abramowitz & Stegun 7.1.26
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-x * x).exp();
    sign * y
}

fn norm_cdf(x: f64) -> f64 {
    0.5 * (1.0 + erf(x * INV_SQRT2))
}

fn norm_pdf(x: f64) -> f64 {
    (-0.5 * x * x).exp() / SQRT_2PI
}

fn d1d2(s: f64, k: f64, t: f64, r: f64, q: f64, sig: f64) -> Option<(f64, f64)> {
    if s <= 0.0 || k <= 0.0 || t <= 0.0 || sig <= 0.0 {
        return None;
    }
    let sqrt_t = t.sqrt();
    let d1 = ((s / k).ln() + (r - q + 0.5 * sig * sig) * t) / (sig * sqrt_t);
    let d2 = d1 - sig * sqrt_t;
    Some((d1, d2))
}

pub fn greeks_put(s: f64, k: f64, t: f64, r: f64, q: f64, sigma: f64) -> Result<Greeks, MlError> {
    let (d1, d2) = d1d2(s, k, t, r, q, sigma).ok_or(MlError::Constraint("bs inputs"))?;
    let df_r = (-r * t).exp();
    let df_q = (-q * t).exp();
    let sqrt_t = t.sqrt();
    let delta = df_q * (norm_cdf(d1) - 1.0);
    let gamma = df_q * norm_pdf(d1) / (s * sigma * sqrt_t);
    let vega = s * df_q * norm_pdf(d1) * sqrt_t;
    let theta = -s * df_q * norm_pdf(d1) * sigma / (2.0 * sqrt_t) + r * k * df_r * norm_cdf(-d2)
        - q * s * df_q * norm_cdf(-d1);
    Ok(Greeks {
        delta,
        gamma,
        theta: theta / 365.0,
        vega: vega / 100.0,
        iv: sigma,
    })
}

pub fn put_price(s: f64, k: f64, t: f64, r: f64, q: f64, sigma: f64) -> Result<f64, MlError> {
    let (d1, d2) = d1d2(s, k, t, r, q, sigma).ok_or(MlError::Constraint("bs inputs"))?;
    let df_r = (-r * t).exp();
    let df_q = (-q * t).exp();
    Ok(k * df_r * norm_cdf(-d2) - s * df_q * norm_cdf(-d1))
}

pub fn implied_vol_put(
    s: f64,
    k: f64,
    t: f64,
    r: f64,
    q: f64,
    mid: f64,
) -> Result<f64, MlError> {
    if mid <= 0.0 {
        return Err(MlError::Constraint("non-positive mid"));
    }
    let mut sigma = 0.2;
    for _ in 0..40 {
        let px = put_price(s, k, t, r, q, sigma)?;
        let g = greeks_put(s, k, t, r, q, sigma)?;
        let vega = g.vega * 100.0;
        let diff = px - mid;
        if diff.abs() < 1e-6 {
            return Ok(sigma);
        }
        if vega.abs() < 1e-12 {
            break;
        }
        sigma = (sigma - diff / vega).clamp(1e-4, 5.0);
    }
    Ok(sigma)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atm_put_delta_negative() {
        let g = greeks_put(500.0, 500.0, 30.0 / 365.0, 0.04, 0.01, 0.2).unwrap();
        assert!(g.delta < 0.0 && g.delta > -0.7);
        let px = put_price(500.0, 500.0, 30.0 / 365.0, 0.04, 0.01, 0.2).unwrap();
        assert!(px > 0.0);
    }
}
