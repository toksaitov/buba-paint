/// Compute the taker fee for a trade.
///
/// `price` is the per-share price (e.g. 0.50 for a 50% probability token).
/// `shares` is the number of outcome tokens traded.
/// `fee_rate` and `exponent` are the market category parameters from Polymarket.
///
/// Returns the total fee in USD.
use crate::config::Config;
use crate::types::MarketWindow;

pub const CRYPTO_FEE_CHANGEOVER_MS: u64 = 1_774_828_800_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeeParams {
    pub fee_rate: f64,
    pub exponent: u32,
}

/// Resolves fee params.
pub fn resolve_fee_params(
    config: &Config,
    market: Option<&MarketWindow>,
    timestamp_ms: u64,
) -> FeeParams {
    if config.taker_fee_override_explicit {
        return FeeParams {
            fee_rate: config.taker_fee_rate,
            exponent: config.taker_fee_exponent,
        };
    }

    if market
        .and_then(|m| m.fee_profile.as_deref())
        .is_some_and(|profile| profile.eq_ignore_ascii_case("crypto"))
    {
        return crypto_fee_params(timestamp_ms);
    }

    FeeParams {
        fee_rate: config.taker_fee_rate,
        exponent: config.taker_fee_exponent,
    }
}

/// Crypto fee params.
pub fn crypto_fee_params(timestamp_ms: u64) -> FeeParams {
    if timestamp_ms >= CRYPTO_FEE_CHANGEOVER_MS {
        FeeParams {
            fee_rate: 0.072,
            exponent: 1,
        }
    } else {
        FeeParams {
            fee_rate: 0.25,
            exponent: 2,
        }
    }
}

/// Compute taker fee.
pub fn compute_taker_fee(price: f64, shares: f64, fee_rate: f64, exponent: u32) -> f64 {
    if price <= 0.0 || price >= 1.0 || shares <= 0.0 || fee_rate <= 0.0 {
        return 0.0;
    }
    let variance = price * (1.0 - price);
    shares * price * fee_rate * variance.powi(i32::try_from(exponent).unwrap_or(2))
}

/// Compute the effective fee rate (fee per dollar spent) for a given price.
///
/// Useful for display and hurdle calculations.
pub fn effective_fee_rate(price: f64, fee_rate: f64, exponent: u32) -> f64 {
    if price <= 0.0 || price >= 1.0 {
        return 0.0;
    }
    let variance = price * (1.0 - price);
    fee_rate * variance.powi(i32::try_from(exponent).unwrap_or(2))
}

/// Spread net edge.
pub fn spread_net_edge(
    up_price: f64,
    down_price: f64,
    shares: f64,
    fee_rate: f64,
    exponent: u32,
) -> f64 {
    if shares <= 0.0 {
        return 0.0;
    }
    let gross = shares * (1.0 - up_price - down_price);
    let fees = compute_taker_fee(up_price, shares, fee_rate, exponent)
        + compute_taker_fee(down_price, shares, fee_rate, exponent);
    gross - fees
}

#[cfg(test)]
#[path = "tests/fees_tests.rs"]
mod tests;
