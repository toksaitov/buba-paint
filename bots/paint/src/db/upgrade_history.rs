use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use futures_util::stream::{self, StreamExt};
use rusqlite::{OptionalExtension, params};

use super::build_data;
use super::database::Database;
use crate::fees::{compute_taker_fee, resolve_fee_params};
use crate::types::MarketWindow;

const DEFAULT_GAMMA_API_URL: &str = "https://gamma-api.polymarket.com";
const DEFAULT_CLOB_API_URL: &str = "https://clob.polymarket.com";
const UPGRADE_KEY: &str = "upgrade-history-v1";
const FETCH_RETRIES: usize = 5;
const FETCH_CONCURRENCY: usize = 16;

#[derive(Clone, Debug)]
struct MarketRow {
    market_id: String,
    condition_id: String,
    slug: String,
    question: String,
    up_token_id: String,
    down_token_id: String,
    end_time: u64,
}

pub struct UpgradeHistoryOptions {
    pub runs_dir: String,
    pub from_run: u32,
    pub to_run: u32,
    pub rebuild_derived: bool,
    pub output: String,
}

/// Upgrade and backfill the selected historical run databases in place.
pub async fn run_upgrade_history(options: UpgradeHistoryOptions) -> anyhow::Result<()> {
    let cache_root = Path::new("data").join("backfill-cache");
    fs::create_dir_all(cache_root.join("gamma"))?;
    fs::create_dir_all(cache_root.join("clob"))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .context("building HTTP client for history upgrade")?;

    for run_number in options.from_run..=options.to_run {
        let run_dir = format!("{run_number:03}");
        let db_path = Path::new(&options.runs_dir)
            .join(&run_dir)
            .join("buba-paint.db");
        if !db_path.exists() {
            println!("[upgrade-history] skip {run_dir}: db missing");
            continue;
        }

        println!("[upgrade-history] upgrading {run_dir}...");
        upgrade_one_db(&client, &db_path, &cache_root).await?;
    }

    if options.rebuild_derived {
        build_data::build_market_data(&options.runs_dir, &options.output)?;
    }

    Ok(())
}

/// Upgrade one run database if the history-upgrade marker is not present yet.
async fn upgrade_one_db(
    client: &reqwest::Client,
    db_path: &Path,
    cache_root: &Path,
) -> anyhow::Result<()> {
    let db_path_str = db_path
        .to_str()
        .context("history DB path is not valid UTF-8")?;
    let db = Database::new(db_path_str)?;
    let conn = db.conn();

    let already_upgraded: Option<i64> = conn
        .query_row(
            "SELECT completed_at_ms FROM history_upgrades WHERE key = ?1",
            params![UPGRADE_KEY],
            |row| row.get(0),
        )
        .optional()?;
    if already_upgraded.is_some() {
        println!(
            "[upgrade-history] {} already upgraded, skipping",
            db_path.display()
        );
        db.close();
        return Ok(());
    }

    backfill_markets(client, conn, cache_root).await?;
    synthesize_feed_events(conn)?;
    backfill_signals(conn)?;
    backfill_trade_audit(conn)?;

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    conn.execute(
        "INSERT OR REPLACE INTO history_upgrades (key, completed_at_ms) VALUES (?1, ?2)",
        params![UPGRADE_KEY, now_ms],
    )?;

    db.close();
    Ok(())
}

/// Backfill enriched market metadata from Gamma and `CLOB` caches.
async fn backfill_markets(
    client: &reqwest::Client,
    conn: &rusqlite::Connection,
    cache_root: &Path,
) -> anyhow::Result<()> {
    let markets = load_market_rows(conn)?;
    let resolved = stream::iter(markets.into_iter().map(|market| {
        let client = client.clone();
        let cache_root = cache_root.to_path_buf();
        async move {
            let (gamma_meta, clob_meta) =
                fetch_market_metadata(&client, &cache_root, &market).await;
            (market, gamma_meta, clob_meta)
        }
    }))
    .buffer_unordered(FETCH_CONCURRENCY)
    .collect::<Vec<_>>()
    .await;

    for (market, gamma_meta, clob_meta) in resolved {
        update_market_metadata(conn, &market, &gamma_meta, &clob_meta)?;
    }

    Ok(())
}

/// Synthesize legacy `feed_events` rows from historical `tick_data`.
fn synthesize_feed_events(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    let existing: i64 = conn.query_row("SELECT COUNT(*) FROM feed_events", [], |row| row.get(0))?;
    if existing > 0 {
        return Ok(());
    }

    conn.execute_batch(
        "INSERT INTO feed_events (
            received_at_ms, event_at_ms, source, event_type, market_id, asset_id,
            price, best_bid, best_ask, bid_size, ask_size, payload_json, fidelity
         )
         SELECT
            timestamp,
            timestamp,
            source,
            CASE source
                WHEN 'binance' THEN 'binance_tick'
                ELSE 'chainlink_price'
            END,
            NULL,
            NULL,
            price,
            bid,
            ask,
            bid_size,
            ask_size,
            NULL,
            'legacy_snapshot'
         FROM tick_data
         WHERE source IN ('binance', 'chainlink');

         INSERT INTO feed_events (
            received_at_ms, event_at_ms, source, event_type, market_id, asset_id,
            price, best_bid, best_ask, bid_size, ask_size, payload_json, fidelity
         )
         SELECT
            t.timestamp,
            t.timestamp,
            t.source,
            'clob_snapshot',
            m.market_id,
            m.up_token_id,
            t.price,
            t.bid,
            t.ask,
            t.bid_size,
            t.ask_size,
            NULL,
            'legacy_snapshot'
         FROM tick_data t
         JOIN markets m
           ON m.start_time <= t.timestamp
          AND m.end_time >= t.timestamp
         WHERE t.source = 'clob_up';

         INSERT INTO feed_events (
            received_at_ms, event_at_ms, source, event_type, market_id, asset_id,
            price, best_bid, best_ask, bid_size, ask_size, payload_json, fidelity
         )
         SELECT
            t.timestamp,
            t.timestamp,
            t.source,
            'clob_snapshot',
            m.market_id,
            m.down_token_id,
            t.price,
            t.bid,
            t.ask,
            t.bid_size,
            t.ask_size,
            NULL,
            'legacy_snapshot'
         FROM tick_data t
         JOIN markets m
           ON m.start_time <= t.timestamp
          AND m.end_time >= t.timestamp
         WHERE t.source = 'clob_down'",
    )?;

    Ok(())
}

/// Backfill signal market ids and replay fidelity on older run DBs.
fn backfill_signals(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "UPDATE signals
         SET market_id = COALESCE(
                market_id,
                (
                    SELECT market_id
                    FROM markets m
                    WHERE m.start_time <= signals.timestamp
                      AND m.end_time >= signals.timestamp
                    ORDER BY m.start_time DESC
                    LIMIT 1
                )
             ),
             execution_fidelity = COALESCE(execution_fidelity, 'legacy_snapshot')
         WHERE market_id IS NULL OR execution_fidelity IS NULL",
    )?;
    Ok(())
}

/// Backfill all legacy trade-execution audit fields in one pass.
fn backfill_trade_audit(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    backfill_trade_defaults(conn)?;
    backfill_missing_signal_ids(conn)?;
    backfill_spread_execution_groups(conn)?;
    backfill_trade_result_fees(conn)?;
    Ok(())
}

/// Populate default execution-audit fields for legacy simulated trades.
fn backfill_trade_defaults(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "UPDATE simulated_trades
         SET requested_price = COALESCE(requested_price, entry_price),
             requested_size = COALESCE(requested_size, size),
             filled_size = COALESCE(filled_size, size),
             avg_fill_price = COALESCE(avg_fill_price, entry_price),
             fill_status = COALESCE(fill_status, 'legacy_assumed_full'),
             fill_reason = COALESCE(fill_reason, 'snapshot_backfill'),
             execution_fidelity = COALESCE(execution_fidelity, 'legacy_snapshot'),
             execution_mode = COALESCE(execution_mode, 'paper'),
             fill_price = COALESCE(fill_price, entry_price)",
    )?;
    Ok(())
}

/// Match legacy trades to the closest persisted signal when missing.
fn backfill_missing_signal_ids(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    let mut trades_stmt = conn.prepare(
        "SELECT id, timestamp, market_id, strategy, side
         FROM simulated_trades
         WHERE signal_id IS NULL",
    )?;
    let trades = trades_stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(trades_stmt);

    for (trade_id, timestamp, market_id, strategy, side) in trades {
        let signal_id: Option<i64> = conn
            .query_row(
                "SELECT id
                 FROM signals
                 WHERE strategy = ?1
                   AND direction = ?2
                   AND COALESCE(market_id, ?3) = ?3
                   AND ABS(timestamp - ?4) <= 1000
                 ORDER BY ABS(timestamp - ?4), id DESC
                 LIMIT 1",
                params![strategy, side, market_id, timestamp],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(signal_id) = signal_id {
            conn.execute(
                "UPDATE simulated_trades SET signal_id = ?1 WHERE id = ?2",
                params![signal_id, trade_id],
            )?;
        }
    }
    Ok(())
}

/// Reconstruct spread execution groups for paired legacy spread trades.
fn backfill_spread_execution_groups(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    let mut spread_stmt = conn.prepare(
        "SELECT id, market_id, strategy, timestamp, side
         FROM simulated_trades
         WHERE strategy = 'spread-capture'
         ORDER BY market_id, strategy, timestamp, id",
    )?;
    let spread_rows = spread_stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(spread_stmt);

    let mut spread_groups: HashMap<(String, String, i64), Vec<(i64, String)>> = HashMap::new();
    for (trade_id, market_id, strategy, timestamp, side) in spread_rows {
        spread_groups
            .entry((market_id, strategy, timestamp))
            .or_default()
            .push((trade_id, side));
    }

    for ((_market_id, _strategy, _timestamp), rows) in spread_groups {
        if rows.len() != 2 {
            continue;
        }
        let has_up = rows.iter().any(|(_, side)| side == "UP");
        let has_down = rows.iter().any(|(_, side)| side == "DOWN");
        if !(has_up && has_down) {
            continue;
        }
        let group_id = format!("legacy-spread-{}", rows[0].0.min(rows[1].0));
        for (trade_id, _) in rows {
            conn.execute(
                "UPDATE simulated_trades
                 SET execution_group_id = COALESCE(execution_group_id, ?1)
                 WHERE id = ?2",
                params![group_id, trade_id],
            )?;
        }
    }
    Ok(())
}

/// Recompute historical trade-result fees under the current schema.
fn backfill_trade_result_fees(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    let mut stmt = conn.prepare(
        "SELECT tr.trade_id, st.entry_price, st.size, st.market_id, m.end_time, m.fee_profile
         FROM trade_results tr
         JOIN simulated_trades st ON st.id = tr.trade_id
         LEFT JOIN markets m ON m.market_id = st.market_id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, f64>(1)?,
            row.get::<_, f64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<u64>>(4)?.unwrap_or(0),
            row.get::<_, Option<String>>(5)?,
        ))
    })?;

    let mut updates = Vec::new();
    for row in rows {
        let (trade_id, entry_price, size, market_id, end_time, fee_profile) = row?;
        let market = MarketWindow {
            market_id,
            question: String::new(),
            up_token_id: String::new(),
            down_token_id: String::new(),
            condition_id: String::new(),
            start_time: end_time.saturating_sub(300_000),
            end_time,
            slug: String::new(),
            outcome: None,
            resolution_source: None,
            fee_profile,
            order_min_size: None,
            order_price_min_tick_size: None,
            maker_base_fee: None,
            taker_base_fee: None,
            rewards_min_size: None,
            rewards_max_spread: None,
        };
        let fee_params =
            resolve_fee_params(&crate::config::Config::default(), Some(&market), end_time);
        let fee_amount =
            compute_taker_fee(entry_price, size, fee_params.fee_rate, fee_params.exponent);
        updates.push((trade_id, fee_amount));
    }
    drop(stmt);

    for (trade_id, fee_amount) in updates {
        conn.execute(
            "UPDATE trade_results
             SET fee_amount = ?1,
                 pnl_net = pnl_0pct - ?1,
                 settlement_status = COALESCE(settlement_status, 'confirmed')
             WHERE trade_id = ?2",
            params![fee_amount, trade_id],
        )?;
    }
    Ok(())
}

/// Load the historical market rows eligible for metadata backfill.
fn load_market_rows(conn: &rusqlite::Connection) -> anyhow::Result<Vec<MarketRow>> {
    let mut stmt = conn.prepare(
        "SELECT market_id, condition_id, slug, question, up_token_id, down_token_id, end_time
         FROM markets
         WHERE slug != ''
         ORDER BY start_time",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(MarketRow {
            market_id: row.get(0)?,
            condition_id: row.get(1)?,
            slug: row.get(2)?,
            question: row.get(3)?,
            up_token_id: row.get(4)?,
            down_token_id: row.get(5)?,
            end_time: row.get(6)?,
        })
    })?;
    let mut markets = Vec::new();
    for row in rows {
        markets.push(row?);
    }
    Ok(markets)
}

/// Fetch the Gamma and `CLOB` metadata needed to enrich one market row.
async fn fetch_market_metadata(
    client: &reqwest::Client,
    cache_root: &Path,
    market: &MarketRow,
) -> (MarketMetadata, MarketMetadata) {
    let gamma_body = match fetch_or_cache_json(
        client,
        &format!("{DEFAULT_GAMMA_API_URL}/events/slug/{}", market.slug),
        &cache_root
            .join("gamma")
            .join(format!("{}.json", market.slug)),
    )
    .await
    {
        Ok(body) => body,
        Err(err) => {
            eprintln!(
                "[upgrade-history] warning: failed to fetch Gamma market {}: {err:#}",
                market.slug
            );
            serde_json::Value::Null
        }
    };

    let gamma_meta = extract_gamma_metadata(
        &gamma_body,
        &market.question,
        &market.up_token_id,
        &market.down_token_id,
    );
    let clob_meta =
        if gamma_meta.order_min_size.is_none() || gamma_meta.order_price_min_tick_size.is_none() {
            fetch_clob_metadata(client, cache_root, market).await
        } else {
            MarketMetadata::default()
        };
    (gamma_meta, clob_meta)
}

/// Fetch fallback `CLOB` metadata for one market row.
async fn fetch_clob_metadata(
    client: &reqwest::Client,
    cache_root: &Path,
    market: &MarketRow,
) -> MarketMetadata {
    match fetch_or_cache_json(
        client,
        &format!("{DEFAULT_CLOB_API_URL}/markets/{}", market.condition_id),
        &cache_root
            .join("clob")
            .join(format!("{}.json", market.condition_id)),
    )
    .await
    {
        Ok(body) => extract_clob_metadata(&body),
        Err(err) => {
            eprintln!(
                "[upgrade-history] warning: failed to fetch CLOB market {}: {err:#}",
                market.condition_id
            );
            MarketMetadata::default()
        }
    }
}

/// Persist the merged market metadata back into the historical DB.
fn update_market_metadata(
    conn: &rusqlite::Connection,
    market: &MarketRow,
    gamma_meta: &MarketMetadata,
    clob_meta: &MarketMetadata,
) -> anyhow::Result<()> {
    let market_window = MarketWindow {
        market_id: market.market_id.clone(),
        question: market.question.clone(),
        up_token_id: market.up_token_id.clone(),
        down_token_id: market.down_token_id.clone(),
        condition_id: market.condition_id.clone(),
        start_time: market.end_time.saturating_sub(300_000),
        end_time: market.end_time,
        slug: market.slug.clone(),
        outcome: conn
            .query_row(
                "SELECT COALESCE(outcome, polymarket_outcome) FROM markets WHERE market_id = ?1",
                params![market.market_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten(),
        resolution_source: gamma_meta.resolution_source.clone(),
        fee_profile: Some("crypto".to_string()),
        order_min_size: gamma_meta.order_min_size.or(clob_meta.order_min_size),
        order_price_min_tick_size: gamma_meta
            .order_price_min_tick_size
            .or(clob_meta.order_price_min_tick_size),
        maker_base_fee: gamma_meta.maker_base_fee.or(clob_meta.maker_base_fee),
        taker_base_fee: gamma_meta.taker_base_fee.or(clob_meta.taker_base_fee),
        rewards_min_size: gamma_meta.rewards_min_size.or(clob_meta.rewards_min_size),
        rewards_max_spread: gamma_meta
            .rewards_max_spread
            .or(clob_meta.rewards_max_spread),
    };
    conn.execute(
        "UPDATE markets
         SET outcome = COALESCE(outcome, polymarket_outcome),
             resolution_source = COALESCE(?1, resolution_source),
             fee_profile = COALESCE(?2, fee_profile),
             order_min_size = COALESCE(?3, order_min_size),
             order_price_min_tick_size = COALESCE(?4, order_price_min_tick_size),
             maker_base_fee = COALESCE(?5, maker_base_fee),
             taker_base_fee = COALESCE(?6, taker_base_fee),
             rewards_min_size = COALESCE(?7, rewards_min_size),
             rewards_max_spread = COALESCE(?8, rewards_max_spread)
         WHERE market_id = ?9",
        params![
            market_window.resolution_source,
            market_window.fee_profile,
            market_window.order_min_size,
            market_window.order_price_min_tick_size,
            market_window.maker_base_fee,
            market_window.taker_base_fee,
            market_window.rewards_min_size,
            market_window.rewards_max_spread,
            market.market_id,
        ],
    )?;
    Ok(())
}

#[derive(Default)]
struct MarketMetadata {
    resolution_source: Option<String>,
    order_min_size: Option<f64>,
    order_price_min_tick_size: Option<f64>,
    maker_base_fee: Option<f64>,
    taker_base_fee: Option<f64>,
    rewards_min_size: Option<f64>,
    rewards_max_spread: Option<f64>,
}

/// Extract market metadata from a Gamma event payload.
fn extract_gamma_metadata(
    body: &serde_json::Value,
    question: &str,
    up_token_id: &str,
    down_token_id: &str,
) -> MarketMetadata {
    let Some(markets) = body.get("markets").and_then(|value| value.as_array()) else {
        return MarketMetadata::default();
    };

    let market = markets
        .iter()
        .find(|market| {
            market
                .get("clobTokenIds")
                .and_then(|value| value.as_array())
                .is_some_and(|tokens| {
                    let values: Vec<_> = tokens.iter().filter_map(|token| token.as_str()).collect();
                    values.contains(&up_token_id) && values.contains(&down_token_id)
                })
                || market.get("question").and_then(|value| value.as_str()) == Some(question)
        })
        .or_else(|| markets.first());

    let Some(market) = market else {
        return MarketMetadata::default();
    };

    MarketMetadata {
        resolution_source: market
            .get("resolutionSource")
            .or_else(|| body.get("resolutionSource"))
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        order_min_size: parse_numeric(
            market
                .get("orderMinSize")
                .or_else(|| body.get("orderMinSize")),
        ),
        order_price_min_tick_size: parse_numeric(
            market
                .get("orderPriceMinTickSize")
                .or_else(|| body.get("orderPriceMinTickSize")),
        ),
        maker_base_fee: parse_numeric(
            market
                .get("makerBaseFee")
                .or_else(|| body.get("makerBaseFee")),
        ),
        taker_base_fee: parse_numeric(
            market
                .get("takerBaseFee")
                .or_else(|| body.get("takerBaseFee")),
        ),
        rewards_min_size: parse_numeric(
            market
                .get("rewardsMinSize")
                .or_else(|| body.get("rewardsMinSize")),
        ),
        rewards_max_spread: parse_numeric(
            market
                .get("rewardsMaxSpread")
                .or_else(|| body.get("rewardsMaxSpread")),
        ),
    }
}

/// Extract market metadata from a `CLOB` market payload.
fn extract_clob_metadata(body: &serde_json::Value) -> MarketMetadata {
    MarketMetadata {
        resolution_source: None,
        order_min_size: parse_numeric(
            body.get("minimum_order_size")
                .or_else(|| body.get("min_order_size")),
        ),
        order_price_min_tick_size: parse_numeric(
            body.get("minimum_tick_size")
                .or_else(|| body.get("tick_size")),
        ),
        maker_base_fee: parse_numeric(body.get("maker_base_fee")),
        taker_base_fee: parse_numeric(body.get("taker_base_fee")),
        rewards_min_size: parse_numeric(
            body.get("rewards")
                .and_then(|rewards| rewards.get("min_size"))
                .or_else(|| body.get("rewards_min_size")),
        ),
        rewards_max_spread: parse_numeric(
            body.get("rewards")
                .and_then(|rewards| rewards.get("max_spread"))
                .or_else(|| body.get("rewards_max_spread")),
        ),
    }
}

/// Parse a numeric `JSON` field that may be encoded as a string.
fn parse_numeric(value: Option<&serde_json::Value>) -> Option<f64> {
    value.and_then(|value| {
        value
            .as_f64()
            .or_else(|| value.as_i64().map(|n| n as f64))
            .or_else(|| value.as_str().and_then(|s| s.parse::<f64>().ok()))
    })
}

/// Fetch `JSON` from the network or reuse the cached copy on disk.
async fn fetch_or_cache_json(
    client: &reqwest::Client,
    url: &str,
    cache_path: &PathBuf,
) -> anyhow::Result<serde_json::Value> {
    if cache_path.exists() {
        let cached = fs::read_to_string(cache_path)
            .with_context(|| format!("reading cache file {}", cache_path.display()))?;
        return serde_json::from_str(&cached)
            .with_context(|| format!("parsing cache file {}", cache_path.display()));
    }

    let mut last_error = None;
    for attempt in 1..=FETCH_RETRIES {
        match client.get(url).send().await {
            Ok(response) if response.status().is_success() => {
                let body = response.text().await?;
                if let Some(parent) = cache_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(cache_path, &body)
                    .with_context(|| format!("writing cache file {}", cache_path.display()))?;
                return serde_json::from_str(&body)
                    .with_context(|| format!("parsing response from {url}"));
            }
            Ok(response) => {
                let status = response.status();
                if status.is_client_error() && status != reqwest::StatusCode::TOO_MANY_REQUESTS {
                    bail!("HTTP {status} for {url}");
                }
                last_error = Some(anyhow::anyhow!("HTTP {status} for {url}"));
            }
            Err(err) => {
                last_error = Some(anyhow::Error::new(err));
            }
        }

        if attempt < FETCH_RETRIES {
            tokio::time::sleep(std::time::Duration::from_millis(
                500 * u64::try_from(attempt).unwrap_or(1),
            ))
            .await;
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("failed to fetch {url}")))
}

#[cfg(test)]
#[path = "tests/upgrade_history_tests.rs"]
mod tests;
