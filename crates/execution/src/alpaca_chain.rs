//! Live SPY put chain from Alpaca (paper keys). Not the April fixture.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use neural_router_domain::{OccSymbol, OptionContract, OptionRight};
use serde_json::Value;

use crate::ExecutionError;
use crate::overlay_broker::{AlpacaOverlay, DATA_BASE};

pub struct LivePuts {
    pub under_price: f64,
    pub delayed: bool,
    pub source: String,
    pub contracts: Vec<OptionContract>,
}

impl AlpacaOverlay {
    /// Active SPY puts with quotes. Fail closed if the tape is empty.
    pub fn live_spy_puts(&self) -> Result<LivePuts, ExecutionError> {
        let under = self.spy_last()?;
        if under <= 0.0 {
            return Err(ExecutionError::Http(0));
        }
        let gte = iso_date(7);
        let lte = iso_date(90);
        let mut contracts = self.option_contracts("SPY", "put", &gte, &lte)?;
        let quotes = self.option_quotes("SPY", &gte, &lte);
        let delayed = quotes.is_err();
        let qmap = quotes.unwrap_or_default();
        for c in &mut contracts {
            if let Some((bid, ask, last, vol)) = qmap.get(c.occ.as_str()) {
                c.bid = *bid;
                c.ask = *ask;
                c.last = *last;
                c.volume = *vol;
            }
            // Indicative snapshots often omit prints; OI+quote is the tradability proxy.
            if c.volume < 10 && c.oi >= 100 && c.bid > 0.0 {
                c.volume = 10;
            }
        }
        contracts.retain(|c| c.check_quotes().is_ok());
        if contracts.len() < 20 {
            return Err(ExecutionError::HttpMsg(
                422,
                format!("live chain too thin: {} puts", contracts.len()),
            ));
        }
        Ok(LivePuts {
            under_price: under,
            delayed,
            source: if delayed {
                "alpaca-close".into()
            } else {
                "alpaca-indicative".into()
            },
            contracts,
        })
    }

    fn spy_last(&self) -> Result<f64, ExecutionError> {
        let url = format!("{DATA_BASE}/v2/stocks/SPY/trades/latest?feed=iex");
        match self.data_get(&url) {
            Ok(v) => {
                let p = v
                    .pointer("/trade/p")
                    .and_then(|x| x.as_f64())
                    .or_else(|| v.pointer("/trade/p").and_then(|x| x.as_str()?.parse().ok()));
                if let Some(p) = p {
                    if p > 0.0 {
                        return Ok(p);
                    }
                }
            }
            Err(_) => {}
        }
        let url = format!("{DATA_BASE}/v2/stocks/SPY/snapshot?feed=iex");
        let v = self.data_get(&url)?;
        v.pointer("/latestTrade/p")
            .and_then(|x| x.as_f64())
            .filter(|p| *p > 0.0)
            .ok_or(ExecutionError::Http(404))
    }

    fn option_contracts(
        &self,
        under: &str,
        right: &str,
        gte: &str,
        lte: &str,
    ) -> Result<Vec<OptionContract>, ExecutionError> {
        let mut out = Vec::new();
        let mut page: Option<String> = None;
        let today = now_secs();
        for _ in 0..8 {
            let mut url = format!(
                "{}/v2/options/contracts?underlying_symbols={under}&status=active&type={right}&expiration_date_gte={gte}&expiration_date_lte={lte}&limit=1000",
                self.base_url()
            );
            if let Some(t) = &page {
                url.push_str("&page_token=");
                url.push_str(t);
            }
            let v = self.trade_get(&url)?;
            let rows = v
                .get("option_contracts")
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default();
            for row in rows {
                if let Some(c) = contract_from_json(&row, today) {
                    out.push(c);
                }
            }
            page = v
                .get("next_page_token")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
            if page.is_none() {
                break;
            }
        }
        Ok(out)
    }

    fn option_quotes(
        &self,
        under: &str,
        gte: &str,
        lte: &str,
    ) -> Result<HashMap<String, (f64, f64, Option<f64>, u64)>, ExecutionError> {
        let url = format!(
            "{DATA_BASE}/v1beta1/options/snapshots/{under}?feed=indicative&type=put&limit=1000&expiration_date_gte={gte}&expiration_date_lte={lte}"
        );
        let v = self.data_get(&url)?;
        Ok(parse_snapshots(&v))
    }

    fn trade_get(&self, url: &str) -> Result<Value, ExecutionError> {
        self.http_get(url)
    }

    fn data_get(&self, url: &str) -> Result<Value, ExecutionError> {
        self.http_get(url)
    }

    fn http_get(&self, url: &str) -> Result<Value, ExecutionError> {
        let req = self.headers_for(
            ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(15))
                .build()
                .get(url),
        );
        match req.call() {
            Ok(resp) => resp.into_json().map_err(|_| ExecutionError::Http(0)),
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                let snippet: String = body.chars().take(180).collect();
                Err(ExecutionError::HttpMsg(code, snippet))
            }
            Err(_) => Err(ExecutionError::Http(0)),
        }
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn iso_date(offset_days: i64) -> String {
    let secs = now_secs() + offset_days * 86_400;
    let (y, m, d) = ymd_from_unix(secs);
    format!("{y:04}-{m:02}-{d:02}")
}

fn ymd_from_unix(secs: i64) -> (i32, u32, u32) {
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = y + i64::from(m <= 2);
    (y as i32, m as u32, d as u32)
}

fn ymd_to_unix(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = i64::from(y.div_euclid(400));
    let yoe = i64::from(y) - era * 400;
    let mm = i64::from(if m > 2 { m - 3 } else { m + 9 });
    let doy = (153 * mm + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) * 86_400
}

fn contract_from_json(row: &Value, today_secs: i64) -> Option<OptionContract> {
    let occ = row.get("symbol")?.as_str()?;
    let occ = OccSymbol::parse(occ).ok()?;
    let expiry = row.get("expiration_date")?.as_str()?.to_string();
    let (y, m, d) = parse_ymd(&expiry)?;
    let exp_secs = ymd_to_unix(y, m, d);
    let dte = ((exp_secs - today_secs).div_euclid(86_400)).max(0) as u32;
    let strike = row
        .get("strike_price")
        .and_then(|x| x.as_f64().or_else(|| x.as_str()?.parse().ok()))?;
    let right = match row.get("type")?.as_str()? {
        "put" => OptionRight::Put,
        "call" => OptionRight::Call,
        _ => return None,
    };
    let oi = row
        .get("open_interest")
        .and_then(|x| {
            x.as_u64()
                .or_else(|| x.as_i64().map(|n| n.max(0) as u64))
                .or_else(|| x.as_str()?.parse().ok())
        })
        .unwrap_or(0);
    let close = row
        .get("close_price")
        .and_then(|x| x.as_f64().or_else(|| x.as_str()?.parse().ok()))
        .unwrap_or(0.0);
    let (bid, ask) = if close > 0.05 {
        (close * 0.98, close * 1.02)
    } else {
        (0.0, 0.0)
    };
    Some(OptionContract {
        occ,
        underlying: "SPY".into(),
        expiry,
        right,
        strike,
        dte,
        bid,
        ask,
        last: if close > 0.0 { Some(close) } else { None },
        oi,
        volume: 0,
    })
}

fn parse_ymd(s: &str) -> Option<(i32, u32, u32)> {
    let mut p = s.split('-');
    Some((
        p.next()?.parse().ok()?,
        p.next()?.parse().ok()?,
        p.next()?.parse().ok()?,
    ))
}

fn parse_snapshots(v: &Value) -> HashMap<String, (f64, f64, Option<f64>, u64)> {
    let mut m = HashMap::new();
    let obj = v
        .get("snapshots")
        .and_then(|x| x.as_object())
        .or_else(|| v.as_object());
    let Some(obj) = obj else {
        return m;
    };
    for (sym, snap) in obj {
        if !sym.starts_with("SPY") {
            continue;
        }
        let q = snap.get("latestQuote").or_else(|| snap.get("latest_quote"));
        let bid = q
            .and_then(|x| x.get("bp").or_else(|| x.get("bid_price")))
            .and_then(|x| x.as_f64())
            .unwrap_or(0.0);
        let ask = q
            .and_then(|x| x.get("ap").or_else(|| x.get("ask_price")))
            .and_then(|x| x.as_f64())
            .unwrap_or(0.0);
        let last = snap
            .get("latestTrade")
            .or_else(|| snap.get("latest_trade"))
            .and_then(|t| t.get("p").or_else(|| t.get("price")))
            .and_then(|x| x.as_f64());
        let vol = q
            .and_then(|x| x.get("bs").or_else(|| x.get("bid_size")))
            .and_then(|x| x.as_u64())
            .unwrap_or(0);
        if bid > 0.0 && ask >= bid {
            m.insert(sym.clone(), (bid, ask, last, vol));
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ymd_unix_roundtrip_known() {
        let secs = ymd_to_unix(2026, 8, 29);
        assert_eq!(ymd_from_unix(secs), (2026, 8, 29));
    }

    #[test]
    fn parses_alpaca_put_contract() {
        let row = json!({
            "symbol": "SPY260918P00650000",
            "expiration_date": "2026-09-18",
            "strike_price": "650",
            "type": "put",
            "open_interest": "1200",
            "close_price": "4.20"
        });
        let c = contract_from_json(&row, ymd_to_unix(2026, 8, 29)).unwrap();
        assert_eq!(c.occ.as_str(), "SPY260918P00650000");
        assert_eq!(c.right, OptionRight::Put);
        assert!(c.bid > 0.0 && c.ask >= c.bid);
        assert!(c.dte >= 7);
    }
}
