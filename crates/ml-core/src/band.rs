#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandStatus {
    Hold,
    Breach,
}

pub fn dollar_delta_stock(shares: f64, price: f64) -> f64 {
    shares * price
}

pub fn band_status(delta: f64, lo: f64, hi: f64) -> BandStatus {
    if delta >= lo && delta <= hi {
        BandStatus::Hold
    } else {
        BandStatus::Breach
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_inside_band() {
        assert_eq!(band_status(0.0, -1_000.0, 1_000.0), BandStatus::Hold);
    }

    #[test]
    fn breach_outside_band() {
        let d = dollar_delta_stock(1_000.0, 500.0);
        assert_eq!(band_status(d, -10_000.0, 10_000.0), BandStatus::Breach);
    }
}
