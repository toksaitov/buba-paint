// Dynamic taker fee computation for Polymarket.
//
// Formula: fee = shares * price * fee_rate * (price * (1 - price))^exponent
//
// Current crypto parameters (until Mar 29 2026): fee_rate=0.25, exponent=2, peak ~1.56% at $0.50.
// New crypto parameters (from Mar 30 2026): fee_rate=0.072, exponent=1, peak ~1.80% at $0.50.

/// Compute the taker fee for a trade.
///
/// `price` is the per-share price (e.g. 0.50 for a 50% probability token).
/// `shares` is the number of outcome tokens traded.
/// `fee_rate` and `exponent` are the market category parameters from Polymarket.
///
/// Returns the total fee in USD.
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

#[cfg(test)]
#[path = "tests/fees_tests.rs"]
mod tests;
