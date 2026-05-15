use anyhow::{Context, bail};
use rusqlite::{Connection, OpenFlags, params};
use serde::Serialize;

use crate::backtest::replay_quality::{self, ReplayQualityReport};
use crate::db::clob_replay_blocks;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveFidelityClass {
    ResearchGradeLive,
    DescriptiveOnlyLive,
    NoLiveTrading,
}

impl LiveFidelityClass {
    /// Return the stable label for one live-fidelity class.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ResearchGradeLive => "research_grade_live",
            Self::DescriptiveOnlyLive => "descriptive_only_live",
            Self::NoLiveTrading => "no_live_trading",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveFidelityRequirement {
    pub key: String,
    pub description: String,
    pub observed: i64,
    pub required: bool,
    pub satisfied: bool,
}

impl LiveFidelityRequirement {
    /// Return one satisfied positive-count requirement.
    #[must_use]
    fn positive(key: &str, description: &str, observed: i64) -> Self {
        Self {
            key: key.to_string(),
            description: description.to_string(),
            observed,
            required: true,
            satisfied: observed > 0,
        }
    }

    /// Return one satisfied zero-bad-count requirement.
    #[must_use]
    fn no_bad_rows(key: &str, description: &str, observed: i64) -> Self {
        Self {
            key: key.to_string(),
            description: description.to_string(),
            observed,
            required: true,
            satisfied: observed == 0,
        }
    }

    /// Return one explicit boolean requirement.
    #[must_use]
    fn boolean(key: &str, description: &str, satisfied: bool) -> Self {
        Self {
            key: key.to_string(),
            description: description.to_string(),
            observed: i64::from(satisfied),
            required: true,
            satisfied,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveFidelityReport {
    pub class: LiveFidelityClass,
    pub start_time: u64,
    pub end_time: u64,
    pub replay_quality_class: String,
    pub replay_missing_required: Vec<String>,
    pub live_session_count: i64,
    pub live_order_intent_count: i64,
    pub live_order_count: i64,
    pub live_fill_count: i64,
    pub requirements: Vec<LiveFidelityRequirement>,
}

impl LiveFidelityReport {
    /// Return whether the interval is valid for real-money research sweeps.
    #[must_use]
    pub fn is_research_grade_live(&self) -> bool {
        self.class == LiveFidelityClass::ResearchGradeLive
    }

    /// Return all missing required private live-fidelity checks.
    #[must_use]
    pub fn missing_required(&self) -> Vec<&LiveFidelityRequirement> {
        self.requirements
            .iter()
            .filter(|requirement| requirement.required && !requirement.satisfied)
            .collect()
    }

    /// Return stable keys for missing required private live-fidelity checks.
    #[must_use]
    pub fn missing_required_keys(&self) -> Vec<String> {
        self.missing_required()
            .into_iter()
            .map(|requirement| requirement.key.clone())
            .collect()
    }
}

/// Analyze live-fidelity for one database path and interval.
pub fn analyze_path(
    db_path: &str,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<LiveFidelityReport> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening live-fidelity DB: {db_path}"))?;
    analyze_connection(&conn, start_time, end_time)
}

/// Analyze live-fidelity for one open database connection and interval.
pub fn analyze_connection(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<LiveFidelityReport> {
    let replay = replay_quality::analyze_connection(conn, start_time, end_time)?;
    let live_session_count = count_live_sessions(conn, start_time, end_time)?;
    let live_order_intent_count = count_rows_in_range(
        conn,
        "live_order_intents",
        "created_at_ms",
        start_time,
        end_time,
    )?;
    let live_order_count =
        count_rows_in_range(conn, "live_orders", "updated_at_ms", start_time, end_time)?;
    let live_fill_count =
        count_rows_in_range(conn, "live_fills", "filled_at_ms", start_time, end_time)?;
    let has_live_evidence =
        live_session_count > 0 || live_order_intent_count > 0 || live_order_count > 0;
    let requirements = if has_live_evidence {
        live_requirements(conn, start_time, end_time, &replay)?
    } else {
        Vec::new()
    };
    let class = classify(has_live_evidence, &requirements);
    Ok(LiveFidelityReport {
        class,
        start_time,
        end_time,
        replay_quality_class: replay.class.as_str().to_string(),
        replay_missing_required: replay
            .missing_required_keys()
            .into_iter()
            .map(str::to_string)
            .collect(),
        live_session_count,
        live_order_intent_count,
        live_order_count,
        live_fill_count,
        requirements,
    })
}

/// Fail unless one database interval is valid for live-money research sweeps.
pub fn validate_live_sweep_input(
    db_path: &str,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<LiveFidelityReport> {
    let report = analyze_path(db_path, start_time, end_time)?;
    match report.class {
        LiveFidelityClass::ResearchGradeLive | LiveFidelityClass::NoLiveTrading => Ok(report),
        LiveFidelityClass::DescriptiveOnlyLive => bail!("{}", blocking_error(&report)),
    }
}

/// Return a printable multiline live-fidelity report.
#[must_use]
pub fn format_report(report: &LiveFidelityReport) -> String {
    let mut lines = vec![
        format!("live_fidelity={}", report.class.as_str()),
        format!("start_time={}", report.start_time),
        format!("end_time={}", report.end_time),
        format!("replay_quality={}", report.replay_quality_class),
        format!(
            "replay_missing_required={}",
            report.replay_missing_required.join(",")
        ),
        format!("live_session_count={}", report.live_session_count),
        format!("live_order_intent_count={}", report.live_order_intent_count),
        format!("live_order_count={}", report.live_order_count),
        format!("live_fill_count={}", report.live_fill_count),
    ];
    for requirement in &report.requirements {
        lines.push(format!(
            "requirement={} satisfied={} observed={} description={}",
            requirement.key, requirement.satisfied, requirement.observed, requirement.description
        ));
    }
    lines.join("\n")
}

/// Return a concise blocking error for non-research-grade live inputs.
#[must_use]
pub fn blocking_error(report: &LiveFidelityReport) -> String {
    let missing = report.missing_required_keys();
    if missing.is_empty() {
        return format!(
            "input DB is not research-grade live: live_fidelity={}",
            report.class.as_str()
        );
    }
    format!(
        "input DB is not research-grade live: live_fidelity={} missing={}",
        report.class.as_str(),
        missing.join(",")
    )
}

/// Return all private live-fidelity requirements for one interval.
fn live_requirements(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
    replay: &ReplayQualityReport,
) -> anyhow::Result<Vec<LiveFidelityRequirement>> {
    Ok(vec![
        LiveFidelityRequirement::boolean(
            "public_replay_sweep_grade",
            "public feed replay quality is sweep_grade",
            replay.is_sweep_grade(),
        ),
        LiveFidelityRequirement::positive(
            "live_trading_session",
            "live_trading session exists in interval",
            count_live_sessions(conn, start_time, end_time)?,
        ),
        LiveFidelityRequirement::positive(
            "live_account_snapshots",
            "live account snapshots exist in interval",
            count_rows_in_range(
                conn,
                "live_account_snapshots",
                "timestamp_ms",
                start_time,
                end_time,
            )?,
        ),
        LiveFidelityRequirement::positive(
            "live_control_audit",
            "live control audit evidence exists in interval",
            count_live_control_audit(conn, start_time, end_time)?,
        ),
        LiveFidelityRequirement::no_bad_rows(
            "no_critical_reconciliation",
            "no critical reconciliation events remain unresolved in interval",
            count_critical_reconciliation(conn, start_time, end_time)?,
        ),
        LiveFidelityRequirement::no_bad_rows(
            "no_unknown_order_state",
            "no unknown live order state remains in interval",
            count_unknown_order_state(conn, start_time, end_time)?,
        ),
        LiveFidelityRequirement::no_bad_rows(
            "signal_feature_snapshots",
            "each live order intent has a raw_event_full signal feature snapshot",
            count_intents_missing_signal_features(conn, start_time, end_time)?,
        ),
        LiveFidelityRequirement::no_bad_rows(
            "order_request_fields",
            "each live order intent has request, fee, and amount metadata",
            count_intents_missing_request_fields(conn, start_time, end_time)?,
        ),
        LiveFidelityRequirement::no_bad_rows(
            "venue_order_fields",
            "each live venue order has client, token, status, timing, and market metadata",
            count_orders_missing_venue_fields(conn, start_time, end_time)?,
        ),
        LiveFidelityRequirement::no_bad_rows(
            "order_book_explainability",
            "each live order is explainable against recorded CLOB top-of-book state",
            count_orders_without_book_explainability(conn, start_time, end_time)?,
        ),
        LiveFidelityRequirement::no_bad_rows(
            "confirmed_fill_lifecycle",
            "filled live orders have confirmed venue trade recovery",
            count_filled_orders_without_confirmed_trade(conn, start_time, end_time)?,
        ),
        LiveFidelityRequirement::no_bad_rows(
            "account_transition_coverage",
            "each live order has account snapshots before and after its lifecycle update",
            count_orders_without_account_transition(conn, start_time, end_time)?,
        ),
        LiveFidelityRequirement::no_bad_rows(
            "redemption_lifecycle",
            "redemptions are either absent or terminally explained",
            count_incomplete_redemptions(conn, start_time, end_time)?,
        ),
    ])
}

/// Classify one interval from live evidence and private requirements.
fn classify(
    has_live_evidence: bool,
    requirements: &[LiveFidelityRequirement],
) -> LiveFidelityClass {
    if !has_live_evidence {
        return LiveFidelityClass::NoLiveTrading;
    }
    if requirements.iter().all(|requirement| requirement.satisfied) {
        LiveFidelityClass::ResearchGradeLive
    } else {
        LiveFidelityClass::DescriptiveOnlyLive
    }
}

/// Count live-trading sessions overlapping one interval.
fn count_live_sessions(conn: &Connection, start_time: u64, end_time: u64) -> anyhow::Result<i64> {
    if !table_exists(conn, "live_sessions")? {
        return Ok(0);
    }
    conn.query_row(
        "SELECT COUNT(*)
         FROM live_sessions
         WHERE execution_mode = 'live_trading'
           AND started_at_ms <= ?2
           AND COALESCE(ended_at_ms, started_at_ms) >= ?1",
        params![
            sqlite_timestamp(start_time, "start_time")?,
            sqlite_timestamp(end_time, "end_time")?
        ],
        |row| row.get(0),
    )
    .context("counting live sessions")
}

/// Count rows in an optional table over one inclusive millisecond interval.
fn count_rows_in_range(
    conn: &Connection,
    table: &str,
    timestamp_column: &str,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<i64> {
    if !table_exists(conn, table)? {
        return Ok(0);
    }
    let sql = format!(
        "SELECT COUNT(*) FROM {table} WHERE {timestamp_column} >= ?1 AND {timestamp_column} <= ?2"
    );
    conn.query_row(
        &sql,
        params![
            sqlite_timestamp(start_time, "start_time")?,
            sqlite_timestamp(end_time, "end_time")?
        ],
        |row| row.get(0),
    )
    .with_context(|| format!("counting {table} rows"))
}

/// Count live control audit rows over one interval.
fn count_live_control_audit(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<i64> {
    if !table_exists(conn, "control_audit")? {
        return Ok(0);
    }
    conn.query_row(
        "SELECT COUNT(*)
         FROM control_audit
         WHERE timestamp_ms >= ?1
           AND timestamp_ms <= ?2
           AND action IN (
             'live_control_requested',
             'live_control_state_changed',
             'live_closeout_exported',
             'live_unknown_order_cancel_all_attempt',
             'live_risk_halt'
           )",
        params![
            sqlite_timestamp(start_time, "start_time")?,
            sqlite_timestamp(end_time, "end_time")?
        ],
        |row| row.get(0),
    )
    .context("counting live control audit")
}

/// Count critical reconciliation events over one interval.
fn count_critical_reconciliation(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<i64> {
    if !table_exists(conn, "live_reconciliation_events")? {
        return Ok(0);
    }
    conn.query_row(
        "SELECT COUNT(*)
         FROM live_reconciliation_events
         WHERE timestamp_ms >= ?1
           AND timestamp_ms <= ?2
           AND severity = 'critical'",
        params![
            sqlite_timestamp(start_time, "start_time")?,
            sqlite_timestamp(end_time, "end_time")?
        ],
        |row| row.get(0),
    )
    .context("counting critical reconciliation")
}

/// Count unknown order states over one interval.
fn count_unknown_order_state(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<i64> {
    if !table_exists(conn, "live_orders")? {
        return Ok(0);
    }
    let unknown_orders: i64 = conn
        .query_row(
            "SELECT COUNT(*)
             FROM live_orders
             WHERE updated_at_ms >= ?1
               AND updated_at_ms <= ?2
               AND status IN (
                 'unknown_submission',
                 'venue_restart',
                 'timeout',
                 'pending_unknown'
               )",
            params![
                sqlite_timestamp(start_time, "start_time")?,
                sqlite_timestamp(end_time, "end_time")?
            ],
            |row| row.get(0),
        )
        .context("counting unknown live orders")?;
    if !table_exists(conn, "live_control_state")? {
        return Ok(unknown_orders);
    }
    let unknown_states: i64 = conn
        .query_row(
            "SELECT COUNT(*)
             FROM live_control_state
             WHERE updated_at_ms >= ?1
               AND updated_at_ms <= ?2
               AND state = 'unknown_order'",
            params![
                sqlite_timestamp(start_time, "start_time")?,
                sqlite_timestamp(end_time, "end_time")?
            ],
            |row| row.get(0),
        )
        .context("counting unknown live control states")?;
    Ok(unknown_orders + unknown_states)
}

/// Count live intents missing raw signal feature snapshots.
fn count_intents_missing_signal_features(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<i64> {
    if !table_exists(conn, "live_order_intents")? {
        return Ok(0);
    }
    conn.query_row(
        "SELECT COUNT(*)
         FROM live_order_intents i
         LEFT JOIN signals s ON s.id = i.signal_id
         LEFT JOIN signal_metrics sm ON sm.signal_id = i.signal_id
         WHERE i.created_at_ms >= ?1
           AND i.created_at_ms <= ?2
           AND (
             i.signal_id IS NULL
             OR s.id IS NULL
             OR sm.signal_id IS NULL
             OR sm.generated_at_ms IS NULL
             OR sm.available_feature_count <= 0
             OR sm.features_json IS NULL
             OR instr(sm.features_json, '\"featureMode\":\"raw_event_full\"') = 0
           )",
        params![
            sqlite_timestamp(start_time, "start_time")?,
            sqlite_timestamp(end_time, "end_time")?
        ],
        |row| row.get(0),
    )
    .context("counting intents missing signal features")
}

/// Count live intents missing request or economic metadata.
fn count_intents_missing_request_fields(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<i64> {
    if !table_exists(conn, "live_order_intents")? {
        return Ok(0);
    }
    conn.query_row(
        "SELECT COUNT(*)
         FROM live_order_intents
         WHERE created_at_ms >= ?1
           AND created_at_ms <= ?2
           AND (
             market_id = ''
             OR strategy = ''
             OR side NOT IN ('BUY', 'SELL', 'UP', 'DOWN')
             OR order_type NOT IN ('FOK', 'FAK')
             OR requested_price IS NULL
             OR requested_size IS NULL
             OR limit_price IS NULL
             OR fee_schedule_json IS NULL
             OR token_fee_rates_json IS NULL
             OR details_json IS NULL
             OR instr(details_json, '\"amount_usd\"') = 0
           )",
        params![
            sqlite_timestamp(start_time, "start_time")?,
            sqlite_timestamp(end_time, "end_time")?
        ],
        |row| row.get(0),
    )
    .context("counting intents missing request fields")
}

/// Count live venue orders missing required persistence fields.
fn count_orders_missing_venue_fields(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<i64> {
    if !table_exists(conn, "live_orders")? {
        return Ok(0);
    }
    conn.query_row(
        "SELECT COUNT(*)
         FROM live_orders
         WHERE updated_at_ms >= ?1
           AND updated_at_ms <= ?2
           AND (
             client_order_id IS NULL
             OR client_order_id = ''
             OR token_id IS NULL
             OR token_id = ''
             OR side NOT IN ('BUY', 'SELL')
             OR order_type NOT IN ('FOK', 'FAK')
             OR status = ''
             OR updated_at_ms < created_at_ms
             OR requested_price IS NULL
             OR requested_size IS NULL
             OR limit_price IS NULL
             OR details_json IS NULL
             OR instr(details_json, '\"tick_size\"') = 0
             OR instr(details_json, '\"min_order_size\"') = 0
             OR instr(details_json, '\"fee_details\"') = 0
           )",
        params![
            sqlite_timestamp(start_time, "start_time")?,
            sqlite_timestamp(end_time, "end_time")?
        ],
        |row| row.get(0),
    )
    .context("counting orders missing venue fields")
}

/// Count orders that cannot be explained against recorded CLOB state.
fn count_orders_without_book_explainability(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<i64> {
    if !table_exists(conn, "live_orders")? {
        return Ok(0);
    }
    let start_ms = sqlite_timestamp(start_time, "start_time")?;
    let end_ms = sqlite_timestamp(end_time, "end_time")?;
    let mut stmt = conn
        .prepare(
            "SELECT created_at_ms, market_id, token_id, limit_price, order_type, requested_size
             FROM live_orders
             WHERE updated_at_ms >= ?1 AND updated_at_ms <= ?2",
        )
        .context("preparing live-order explainability query")?;
    let rows = stmt
        .query_map(params![start_ms, end_ms], |row| {
            Ok(BookExplainabilityOrder {
                created_at_ms: row.get(0)?,
                market_id: row.get(1)?,
                token_id: row.get(2)?,
                limit_price: row.get(3)?,
                order_type: row.get(4)?,
                requested_size: row.get(5)?,
            })
        })
        .context("executing live-order explainability query")?;
    let mut missing = 0_i64;
    for row in rows {
        let order = row.context("reading live-order explainability row")?;
        if !has_row_book_evidence(conn, start_ms, &order)?
            && !has_block_book_evidence(conn, start_ms, &order)?
        {
            missing = missing.saturating_add(1);
        }
    }
    Ok(missing)
}

#[derive(Debug)]
struct BookExplainabilityOrder {
    created_at_ms: i64,
    market_id: Option<String>,
    token_id: Option<String>,
    limit_price: Option<f64>,
    order_type: String,
    requested_size: Option<f64>,
}

/// Return whether legacy row storage explains one live order.
fn has_row_book_evidence(
    conn: &Connection,
    start_ms: i64,
    order: &BookExplainabilityOrder,
) -> anyhow::Result<bool> {
    if !table_exists(conn, "feed_events")? && !table_exists(conn, "clob_replay_events")? {
        return Ok(false);
    }
    let mut found = false;
    if table_exists(conn, "feed_events")? {
        found |= row_storage_book_evidence(conn, "feed_events", start_ms, order)?;
    }
    if table_exists(conn, "clob_replay_events")? {
        found |= row_storage_book_evidence(conn, "clob_replay_events", start_ms, order)?;
    }
    Ok(found)
}

/// Return whether one row-storage table explains one live order.
fn row_storage_book_evidence(
    conn: &Connection,
    table: &str,
    start_ms: i64,
    order: &BookExplainabilityOrder,
) -> anyhow::Result<bool> {
    let rows: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*)
                 FROM {table}
                 WHERE received_at_ms >= ?1
                   AND received_at_ms <= ?2
                   AND market_id = ?3
                   AND source IN ('clob_up', 'clob_down')
                   AND best_ask IS NOT NULL
                   AND ask_size IS NOT NULL
                   AND (?4 IS NULL OR asset_id = ?4)
                   AND (?5 IS NULL OR best_ask <= ?5)
                   AND (
                     ?6 = 'FAK'
                     OR ?7 IS NULL
                     OR ask_size >= ?7
                   )"
            ),
            params![
                start_ms,
                order.created_at_ms,
                order.market_id.as_deref(),
                order.token_id.as_deref(),
                order.limit_price,
                order.order_type.as_str(),
                order.requested_size,
            ],
            |row| row.get(0),
        )
        .with_context(|| format!("checking {table} book explainability"))?;
    Ok(rows > 0)
}

/// Return whether compressed block storage explains one live order.
fn has_block_book_evidence(
    conn: &Connection,
    start_ms: i64,
    order: &BookExplainabilityOrder,
) -> anyhow::Result<bool> {
    if !table_exists(conn, "clob_replay_blocks")? {
        return Ok(false);
    }
    let mut stmt = conn
        .prepare(
            "SELECT checksum, payload
             FROM clob_replay_blocks
             WHERE max_received_at_ms >= ?1 AND min_received_at_ms <= ?2",
        )
        .context("preparing CLOB block explainability query")?;
    let rows = stmt
        .query_map(params![start_ms, order.created_at_ms], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .context("executing CLOB block explainability query")?;
    for row in rows {
        let (checksum, payload) = row.context("reading CLOB block explainability row")?;
        let events = clob_replay_blocks::decode_payload(&payload, &checksum)
            .context("decoding CLOB block explainability row")?;
        if events
            .into_iter()
            .any(|event| block_event_matches_order(start_ms, order, &event))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Return whether one decoded block event explains one order.
fn block_event_matches_order(
    start_ms: i64,
    order: &BookExplainabilityOrder,
    event: &clob_replay_blocks::DecodedClobReplayEvent,
) -> bool {
    let Ok(received_at_ms) = i64::try_from(event.received_at_ms) else {
        return false;
    };
    if received_at_ms < start_ms || received_at_ms > order.created_at_ms {
        return false;
    }
    if order.market_id.as_deref() != event.market_id.as_deref() {
        return false;
    }
    if let Some(token_id) = order.token_id.as_deref()
        && Some(token_id) != event.asset_id.as_deref()
    {
        return false;
    }
    if let Some(limit_price) = order.limit_price
        && event.best_ask > limit_price
    {
        return false;
    }
    order.order_type == "FAK"
        || order
            .requested_size
            .is_none_or(|requested_size| event.ask_size >= requested_size)
}

/// Count filled orders that lack confirmed trade recovery.
fn count_filled_orders_without_confirmed_trade(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<i64> {
    if !table_exists(conn, "live_orders")? {
        return Ok(0);
    }
    conn.query_row(
        "SELECT COUNT(*)
         FROM live_orders o
         WHERE o.updated_at_ms >= ?1
           AND o.updated_at_ms <= ?2
           AND COALESCE(o.accepted_size, 0) > 0
           AND NOT EXISTS (
             SELECT 1
             FROM live_fills f
             WHERE f.session_id = o.session_id
               AND (f.live_order_id = o.id OR f.intent_id = o.intent_id)
               AND f.venue_trade_id IS NOT NULL
               AND f.status IN ('confirmed', 'confirmed_from_activity')
           )",
        params![
            sqlite_timestamp(start_time, "start_time")?,
            sqlite_timestamp(end_time, "end_time")?
        ],
        |row| row.get(0),
    )
    .context("counting filled orders without confirmed trade recovery")
}

/// Count orders without account snapshots bracketing the lifecycle.
fn count_orders_without_account_transition(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<i64> {
    if !table_exists(conn, "live_orders")? {
        return Ok(0);
    }
    conn.query_row(
        "SELECT COUNT(*)
         FROM live_orders o
         WHERE o.updated_at_ms >= ?1
           AND o.updated_at_ms <= ?2
           AND (
             NOT EXISTS (
               SELECT 1
               FROM live_account_snapshots a
               WHERE a.session_id = o.session_id
                 AND a.timestamp_ms <= o.created_at_ms
             )
             OR NOT EXISTS (
               SELECT 1
               FROM live_account_snapshots a
               WHERE a.session_id = o.session_id
                 AND a.timestamp_ms >= o.updated_at_ms
             )
           )",
        params![
            sqlite_timestamp(start_time, "start_time")?,
            sqlite_timestamp(end_time, "end_time")?
        ],
        |row| row.get(0),
    )
    .context("counting orders without account transitions")
}

/// Count redemptions that are neither absent nor terminally explained.
fn count_incomplete_redemptions(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<i64> {
    if !table_exists(conn, "live_redemptions")? {
        return Ok(0);
    }
    conn.query_row(
        "SELECT COUNT(*)
         FROM live_redemptions
         WHERE detected_redeemable_at_ms >= ?1
           AND detected_redeemable_at_ms <= ?2
           AND status NOT IN ('none', 'confirmed', 'failed', 'not_redeemable')",
        params![
            sqlite_timestamp(start_time, "start_time")?,
            sqlite_timestamp(end_time, "end_time")?
        ],
        |row| row.get(0),
    )
    .context("counting incomplete redemptions")
}

/// Return whether the current connection has one table.
fn table_exists(conn: &Connection, table: &str) -> anyhow::Result<bool> {
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get(0),
        )
        .with_context(|| format!("checking table existence: {table}"))?;
    Ok(exists > 0)
}

/// Convert one replay timestamp into a SQLite-safe signed integer.
fn sqlite_timestamp(timestamp: u64, label: &str) -> anyhow::Result<i64> {
    i64::try_from(timestamp).with_context(|| format!("{label} does not fit in i64"))
}

#[cfg(test)]
#[path = "tests/live_fidelity_tests.rs"]
mod tests;
