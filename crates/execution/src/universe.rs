//! Alpaca US equity universe: listed exchanges, ranked, capped.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::ExecutionError;
use crate::overlay_broker::{AlpacaOverlay, DATA_BASE};

const LISTED: &[&str] = &["NYSE", "NASDAQ", "ARCA", "AMEX", "NYSEARCA"];
pub const UNIVERSE_CAP: usize = 500;
pub const UNIVERSE_MIN: usize = 200;
/// Batches of ≤100 symbols per Alpaca snapshot call (doc: desk-suggest.md).
/// Bounded so a bad asset list can't loop forever — 120 batches covers the
/// full NYSE/NASDAQ/ARCA/AMEX common-stock set with room to spare.
const SNAPSHOT_BATCH: usize = 100;
const MAX_SNAPSHOT_BATCHES: usize = 120;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniverseRow {
    pub symbol: String,
    pub name: String,
    pub exchange: String,
    pub last: f64,
    pub volume: u64,
    pub dollar_volume: f64,
    pub has_options: bool,
    pub shortable: bool,
    pub easy_to_borrow: bool,
    pub score: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AlpacaAsset {
    symbol: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    exchange: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    tradable: bool,
    #[serde(default)]
    shortable: bool,
    #[serde(default)]
    easy_to_borrow: bool,
    #[serde(default)]
    attributes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SnapQuote {
    pub last: f64,
    pub volume: u64,
}

pub fn parse_assets_json(raw: &str) -> Result<Vec<AlpacaAsset>, String> {
    serde_json::from_str(raw).map_err(|e| e.to_string())
}

pub fn filter_listed(assets: &[AlpacaAsset]) -> Vec<AlpacaAsset> {
    assets
        .iter()
        .filter(|a| {
            a.status.eq_ignore_ascii_case("active")
                && a.tradable
                && LISTED.iter().any(|ex| a.exchange.eq_ignore_ascii_case(ex))
                && !a.symbol.contains('.')
                && a.symbol.chars().all(|c| c.is_ascii_alphanumeric())
                && a.symbol.len() <= 6
        })
        .cloned()
        .collect()
}

pub fn rank_universe(assets: Vec<AlpacaAsset>, quotes: &[(String, SnapQuote)]) -> Vec<UniverseRow> {
    let q: std::collections::HashMap<_, _> = quotes.iter().cloned().collect();
    let mut rows: Vec<UniverseRow> = assets
        .into_iter()
        .map(|a| {
            let snap = q.get(&a.symbol);
            let last = snap.map(|s| s.last).unwrap_or(0.0);
            let volume = snap.map(|s| s.volume).unwrap_or(0);
            let dollar_volume = last * volume as f64;
            UniverseRow {
                has_options: a.attributes.iter().any(|x| x == "has_options"),
                shortable: a.shortable,
                easy_to_borrow: a.easy_to_borrow,
                score: dollar_volume,
                symbol: a.symbol,
                name: a.name,
                exchange: a.exchange,
                last,
                volume,
                dollar_volume,
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        b.dollar_volume
            .partial_cmp(&a.dollar_volume)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.symbol.cmp(&b.symbol))
    });
    rows.truncate(UNIVERSE_CAP);
    rows
}

/// Hermetic ≥200 listed names for tests and fixture boot.
pub fn fixture_universe() -> Vec<UniverseRow> {
    let seeds = [
        ("AAPL", "Apple Inc", "NASDAQ"),
        ("MSFT", "Microsoft", "NASDAQ"),
        ("NVDA", "NVIDIA", "NASDAQ"),
        ("TSLA", "Tesla", "NASDAQ"),
        ("AMZN", "Amazon", "NASDAQ"),
        ("GOOG", "Alphabet", "NASDAQ"),
        ("META", "Meta", "NASDAQ"),
        ("SPY", "SPDR S&P 500", "ARCA"),
        ("QQQ", "Invesco QQQ", "NASDAQ"),
        ("IWM", "iShares Russell 2000", "ARCA"),
    ];
    let mut assets = Vec::with_capacity(UNIVERSE_MIN + 20);
    for (i, (sym, name, ex)) in seeds.iter().enumerate() {
        assets.push(asset(sym, name, ex, true));
        let _ = i;
    }
    let mut i = 0;
    while assets.len() < UNIVERSE_MIN + 20 {
        let sym = format!("T{i:03}");
        assets.push(asset(&sym, &format!("Test {i}"), "NASDAQ", true));
        i += 1;
    }
    assets.push(asset("JUNKX", "OTC junk", "OTC", false));
    let listed = filter_listed(&assets);
    let quotes: Vec<(String, SnapQuote)> = listed
        .iter()
        .enumerate()
        .map(|(i, a)| {
            (
                a.symbol.clone(),
                SnapQuote {
                    last: 10.0 + i as f64,
                    volume: 1_000_000 - (i as u64 * 100),
                },
            )
        })
        .collect();
    rank_universe(listed, &quotes)
}

fn asset(symbol: &str, name: &str, exchange: &str, tradable: bool) -> AlpacaAsset {
    AlpacaAsset {
        symbol: symbol.into(),
        name: name.into(),
        exchange: exchange.into(),
        status: "active".into(),
        tradable,
        shortable: true,
        easy_to_borrow: true,
        attributes: vec!["has_options".into()],
    }
}

impl AlpacaOverlay {
    /// Alpaca's own tradable US-equity universe (`GET /v2/assets`), ranked by
    /// dollar volume and capped. Fail closed on thin/broken input — caller
    /// keeps the last-good (or fixture) table, never an empty screen.
    pub fn live_universe(&self) -> Result<Vec<UniverseRow>, ExecutionError> {
        let url = format!(
            "{}/v2/assets?status=active&asset_class=us_equity",
            self.base_url()
        );
        let raw = self.get_text(&url)?;
        let assets = parse_assets_json(&raw).map_err(|e| ExecutionError::HttpMsg(0, e))?;
        let listed = filter_listed(&assets);
        if listed.len() < UNIVERSE_MIN {
            return Err(ExecutionError::HttpMsg(
                422,
                format!("listed universe too thin: {}", listed.len()),
            ));
        }
        let quotes = self.batched_snapshots(&listed);
        Ok(rank_universe(listed, &quotes))
    }

    /// Best-effort last/volume per symbol. A dropped batch just leaves those
    /// rows at dollar_volume 0 (sorted last) — never aborts the whole table.
    fn batched_snapshots(&self, assets: &[AlpacaAsset]) -> Vec<(String, SnapQuote)> {
        let mut out = Vec::with_capacity(assets.len());
        for chunk in assets.chunks(SNAPSHOT_BATCH).take(MAX_SNAPSHOT_BATCHES) {
            let symbols = chunk
                .iter()
                .map(|a| a.symbol.as_str())
                .collect::<Vec<_>>()
                .join(",");
            let url = format!("{DATA_BASE}/v2/stocks/snapshots?symbols={symbols}&feed=iex");
            match self.get_text(&url) {
                Ok(raw) => out.extend(parse_snapshots_json(&raw)),
                Err(e) => tracing::warn!(error = %e, n = chunk.len(), "snapshot batch skipped"),
            }
        }
        out
    }

    fn get_text(&self, url: &str) -> Result<String, ExecutionError> {
        let req = self.headers_for(
            ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(15))
                .build()
                .get(url),
        );
        match req.call() {
            Ok(resp) => resp.into_string().map_err(|_| ExecutionError::Http(0)),
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                let snippet: String = body.chars().take(180).collect();
                Err(ExecutionError::HttpMsg(code, snippet))
            }
            Err(_) => Err(ExecutionError::Http(0)),
        }
    }
}

/// `{"AAPL": {"latestTrade": {"p": ...}, "dailyBar": {"c": ..., "v": ...}}, ...}`
fn parse_snapshots_json(raw: &str) -> Vec<(String, SnapQuote)> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    let Some(obj) = v.as_object() else {
        return Vec::new();
    };
    obj.iter()
        .filter_map(|(sym, snap)| {
            let last = snap
                .pointer("/latestTrade/p")
                .and_then(|x| x.as_f64())
                .filter(|p| *p > 0.0)
                .or_else(|| snap.pointer("/dailyBar/c").and_then(|x| x.as_f64()))?;
            if last <= 0.0 {
                return None;
            }
            let volume = snap
                .pointer("/dailyBar/v")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            Some((sym.clone(), SnapQuote { last, volume }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_alpaca_snapshot_last_and_volume() {
        let raw = r#"{
          "AAPL": {"latestTrade": {"p": 227.5}, "dailyBar": {"c": 226.0, "v": 42000000}},
          "ZZZZ": {"dailyBar": {"c": 0, "v": 100}}
        }"#;
        let q = parse_snapshots_json(raw);
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].0, "AAPL");
        assert!((q[0].1.last - 227.5).abs() < 1e-9);
        assert_eq!(q[0].1.volume, 42_000_000);
    }

    #[test]
    fn fixture_has_at_least_200_listed() {
        let u = fixture_universe();
        assert!(u.len() >= UNIVERSE_MIN, "got {}", u.len());
        assert!(u.len() <= UNIVERSE_CAP);
        assert!(u.iter().any(|r| r.symbol == "AAPL"));
        assert!(u.iter().any(|r| r.symbol == "TSLA"));
        assert!(u.iter().all(|r| r.exchange != "OTC"));
        assert!(u[0].dollar_volume >= u[u.len() - 1].dollar_volume);
    }

    #[test]
    fn drops_otc_and_inactive() {
        let raw = r#"[
          {"symbol":"AAPL","name":"Apple","exchange":"NASDAQ","status":"active","tradable":true,"attributes":["has_options"]},
          {"symbol":"JUNK","name":"Pink","exchange":"OTC","status":"active","tradable":true},
          {"symbol":"DEAD","name":"Dead","exchange":"NYSE","status":"inactive","tradable":true}
        ]"#;
        let parsed = parse_assets_json(raw).unwrap();
        let listed = filter_listed(&parsed);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].symbol, "AAPL");
    }
}
