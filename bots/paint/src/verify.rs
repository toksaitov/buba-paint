use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use rusqlite::params;
use tokio::sync::Semaphore;

/// A market row from the database that needs verification.
#[derive(Debug, Clone)]
pub struct MarketRow {
    pub market_id: String,
    pub slug: String,
    pub our_outcome: Option<String>,
}

/// Result of fetching a single market's resolution from Gamma.
#[derive(Debug, Clone)]
pub struct ResolutionResult {
    pub slug: String,
    pub market_id: String,
    pub polymarket_outcome: Option<String>,
    pub error: Option<String>,
}

/// Summary statistics from the verification run.
#[derive(Debug, Clone)]
pub struct VerificationSummary {
    pub total_markets: usize,
    pub fetched: usize,
    pub not_resolved: usize,
    pub fetch_errors: usize,
    pub agreements: usize,
    pub disagreements: usize,
    pub our_outcome_missing: usize,
}

/// Parse the Gamma API `/events?slug=...` response to extract the outcome.
///
/// Returns `Some("UP")` if outcomePrices is `[1, 0]`, `Some("DOWN")` if
/// `[0, 1]`, or `None` if the market is not yet resolved or the response
/// is unexpected.
pub fn parse_gamma_outcome(body: &serde_json::Value) -> Option<String> {
    let event = if let Some(arr) = body.as_array() {
        arr.first()?
    } else {
        body
    };
    let markets = event.get("markets")?.as_array()?;
    let market = markets.first()?;

    let outcome_prices_val = market.get("outcomePrices")?;
    let prices: Vec<String> = if let Some(arr) = outcome_prices_val.as_array() {
        arr.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()
    } else if let Some(s) = outcome_prices_val.as_str() {
        serde_json::from_str(s).ok()?
    } else {
        return None;
    };

    if prices.len() < 2 {
        return None;
    }

    let up_price: f64 = prices[0].parse().ok()?;
    let down_price: f64 = prices[1].parse().ok()?;

    if (up_price - 1.0).abs() < 0.01 && down_price.abs() < 0.01 {
        Some("UP".to_string())
    } else if up_price.abs() < 0.01 && (down_price - 1.0).abs() < 0.01 {
        Some("DOWN".to_string())
    } else {
        None
    }
}

/// Load all markets from the database that have a slug.
pub fn load_markets(conn: &rusqlite::Connection) -> anyhow::Result<Vec<MarketRow>> {
    let has_outcome_col = conn.prepare("SELECT outcome FROM markets LIMIT 0").is_ok();

    let sql = if has_outcome_col {
        "SELECT market_id, slug, outcome FROM markets WHERE slug != '' ORDER BY start_time"
    } else {
        "SELECT market_id, slug, NULL FROM markets WHERE slug != '' ORDER BY start_time"
    };

    let mut stmt = conn.prepare(sql).context("preparing markets query")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(MarketRow {
                market_id: row.get(0)?,
                slug: row.get(1)?,
                our_outcome: row.get(2)?,
            })
        })
        .context("executing markets query")?;

    let mut markets = Vec::new();
    for row in rows {
        markets.push(row.context("reading market row")?);
    }
    Ok(markets)
}

/// Ensure the `polymarket_outcome` column exists on the markets table.
pub fn ensure_polymarket_outcome_column(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    let result = conn.execute_batch("ALTER TABLE markets ADD COLUMN polymarket_outcome TEXT");
    match result {
        Ok(()) => Ok(()),
        Err(e) if e.to_string().contains("duplicate column") => Ok(()),
        Err(e) => Err(e).context("adding polymarket_outcome column"),
    }
}

/// Store the Polymarket outcome for a single market.
pub fn store_polymarket_outcome(
    conn: &rusqlite::Connection,
    market_id: &str,
    outcome: &str,
) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE markets SET polymarket_outcome = ?1 WHERE market_id = ?2",
        params![outcome, market_id],
    )
    .context("updating polymarket_outcome")?;
    Ok(())
}

/// Fetch the resolution for a single market from the Gamma API.
async fn fetch_one(
    client: &reqwest::Client,
    gamma_url: &str,
    market: &MarketRow,
) -> ResolutionResult {
    let url = format!("{gamma_url}/events/slug/{}", market.slug);

    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            return ResolutionResult {
                slug: market.slug.clone(),
                market_id: market.market_id.clone(),
                polymarket_outcome: None,
                error: Some(format!("request failed: {e}")),
            };
        }
    };

    if !resp.status().is_success() {
        return ResolutionResult {
            slug: market.slug.clone(),
            market_id: market.market_id.clone(),
            polymarket_outcome: None,
            error: Some(format!("HTTP {}", resp.status())),
        };
    }

    let body: serde_json::Value = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            return ResolutionResult {
                slug: market.slug.clone(),
                market_id: market.market_id.clone(),
                polymarket_outcome: None,
                error: Some(format!("JSON parse error: {e}")),
            };
        }
    };

    let outcome = parse_gamma_outcome(&body);

    ResolutionResult {
        slug: market.slug.clone(),
        market_id: market.market_id.clone(),
        polymarket_outcome: outcome,
        error: None,
    }
}

/// Run the full verification: load markets, fetch outcomes, compare, store.
pub async fn run_verify_settlements(
    db_path: &str,
    gamma_url: &str,
    concurrency: usize,
) -> anyhow::Result<VerificationSummary> {
    let conn = rusqlite::Connection::open(db_path).context("opening database")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;

    ensure_polymarket_outcome_column(&conn)?;

    let markets = load_markets(&conn)?;
    let total = markets.len();

    println!("[verify] Loaded {total} markets from {db_path}");
    println!("[verify] Fetching resolutions from Gamma API (concurrency={concurrency})...");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("building HTTP client")?;

    let semaphore = Arc::new(Semaphore::new(concurrency));
    let gamma_url = gamma_url.to_string();

    let mut handles = Vec::with_capacity(total);
    for market in &markets {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let client = client.clone();
        let gamma = gamma_url.clone();
        let m = market.clone();
        handles.push(tokio::spawn(async move {
            let result = fetch_one(&client, &gamma, &m).await;
            drop(permit);
            result
        }));
    }

    let mut results = Vec::with_capacity(total);
    let mut done = 0usize;
    for handle in handles {
        let result = handle.await.context("task panicked")?;
        results.push(result);
        done += 1;
        if done % 500 == 0 || done == total {
            println!("[verify] Progress: {done}/{total}");
        }
    }

    let mut summary = VerificationSummary {
        total_markets: total,
        fetched: 0,
        not_resolved: 0,
        fetch_errors: 0,
        agreements: 0,
        disagreements: 0,
        our_outcome_missing: 0,
    };

    for (i, result) in results.iter().enumerate() {
        if let Some(ref err) = result.error {
            summary.fetch_errors += 1;
            if summary.fetch_errors <= 5 {
                println!("[verify] ERROR {}: {err}", result.slug);
            }
            continue;
        }

        let Some(ref pm_outcome) = result.polymarket_outcome else {
            summary.not_resolved += 1;
            continue;
        };

        summary.fetched += 1;
        store_polymarket_outcome(&conn, &result.market_id, pm_outcome)?;

        let our = &markets[i].our_outcome;
        match our {
            Some(ours) if ours == pm_outcome => summary.agreements += 1,
            Some(_) => summary.disagreements += 1,
            None => summary.our_outcome_missing += 1,
        }
    }

    println!("\n[verify] === SETTLEMENT VERIFICATION SUMMARY ===");
    println!("[verify] Total markets:      {}", summary.total_markets);
    println!("[verify] Fetched outcomes:    {}", summary.fetched);
    println!("[verify] Not resolved:        {}", summary.not_resolved);
    println!("[verify] Fetch errors:        {}", summary.fetch_errors);
    println!("[verify] Agreements:          {}", summary.agreements);
    println!("[verify] Disagreements:       {}", summary.disagreements);
    println!(
        "[verify] Our outcome missing: {}",
        summary.our_outcome_missing
    );

    if summary.agreements + summary.disagreements > 0 {
        let accuracy =
            summary.agreements as f64 / (summary.agreements + summary.disagreements) as f64;
        println!("[verify] Accuracy:            {:.1}%", accuracy * 100.0);
    }

    Ok(summary)
}

#[cfg(test)]
#[path = "tests/verify_tests.rs"]
mod tests;
