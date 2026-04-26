use anyhow::{Context, bail};
use rusqlite::{Connection, OpenFlags, params};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayQualityClass {
    SweepGrade,
    DescriptiveOnly,
    LegacySnapshot,
    Empty,
}

impl ReplayQualityClass {
    /// Return the stable storage quality label.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SweepGrade => "sweep_grade",
            Self::DescriptiveOnly => "descriptive_only",
            Self::LegacySnapshot => "legacy_snapshot",
            Self::Empty => "empty",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayQualityRequirement {
    pub key: &'static str,
    pub description: &'static str,
    pub rows: i64,
    pub required: bool,
}

impl ReplayQualityRequirement {
    /// Return whether this requirement is satisfied.
    #[must_use]
    pub fn present(&self) -> bool {
        self.rows > 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayQualityReport {
    pub class: ReplayQualityClass,
    pub start_time: u64,
    pub end_time: u64,
    pub feed_event_rows: i64,
    pub legacy_tick_rows: i64,
    pub requirements: Vec<ReplayQualityRequirement>,
}

impl ReplayQualityReport {
    /// Return whether the report is good enough for parameter sweeps.
    #[must_use]
    pub fn is_sweep_grade(&self) -> bool {
        self.class == ReplayQualityClass::SweepGrade
    }

    /// Return all missing required feed classes.
    #[must_use]
    pub fn missing_required(&self) -> Vec<&ReplayQualityRequirement> {
        self.requirements
            .iter()
            .filter(|requirement| requirement.required && !requirement.present())
            .collect()
    }
}

/// Analyze replay fidelity for one database and time interval.
pub fn analyze_connection(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<ReplayQualityReport> {
    let feed_event_rows =
        count_rows_in_range(conn, "feed_events", "received_at_ms", start_time, end_time)?;
    let legacy_tick_rows =
        count_rows_in_range(conn, "tick_data", "timestamp", start_time, end_time)?;
    let requirements = if feed_event_rows > 0 {
        vec![
            requirement(
                conn,
                start_time,
                end_time,
                "binance_agg_trade",
                "Binance aggTrade rows with price, trade size, and signed quantity",
                "source = 'binance' AND event_type = 'aggTrade' AND price IS NOT NULL AND trade_size IS NOT NULL AND signed_quantity IS NOT NULL",
            )?,
            requirement(
                conn,
                start_time,
                end_time,
                "binance_book_ticker",
                "Binance bookTicker rows with best bid, best ask, bid size, and ask size",
                "source = 'binance' AND event_type = 'bookTicker' AND best_bid IS NOT NULL AND best_ask IS NOT NULL AND bid_size IS NOT NULL AND ask_size IS NOT NULL",
            )?,
            requirement(
                conn,
                start_time,
                end_time,
                "binance_depth",
                "Binance depth rows with depth notional or imbalance fields",
                "source = 'binance' AND event_type = 'depth' AND ((depth_bid_notional IS NOT NULL AND depth_ask_notional IS NOT NULL) OR depth_imbalance IS NOT NULL)",
            )?,
            requirement(
                conn,
                start_time,
                end_time,
                "chainlink_price",
                "Chainlink price rows",
                "source = 'chainlink' AND event_type = 'chainlink_price' AND price IS NOT NULL",
            )?,
            requirement(
                conn,
                start_time,
                end_time,
                "clob_up_top_of_book",
                "CLOB UP top-of-book rows with best bid and ask",
                "source = 'clob_up' AND best_bid IS NOT NULL AND best_ask IS NOT NULL",
            )?,
            requirement(
                conn,
                start_time,
                end_time,
                "clob_down_top_of_book",
                "CLOB DOWN top-of-book rows with best bid and ask",
                "source = 'clob_down' AND best_bid IS NOT NULL AND best_ask IS NOT NULL",
            )?,
        ]
    } else {
        Vec::new()
    };
    let class = classify(feed_event_rows, legacy_tick_rows, &requirements);
    Ok(ReplayQualityReport {
        class,
        start_time,
        end_time,
        feed_event_rows,
        legacy_tick_rows,
        requirements,
    })
}

/// Analyze replay fidelity for one database path and time interval.
pub fn analyze_path(
    data_path: &str,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<ReplayQualityReport> {
    let conn = Connection::open_with_flags(data_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening data DB: {data_path}"))?;
    analyze_connection(&conn, start_time, end_time)
}

/// Fail unless the database interval is replay-grade enough for a sweep.
pub fn validate_sweep_input(
    data_path: &str,
    start_time: u64,
    end_time: u64,
) -> anyhow::Result<ReplayQualityReport> {
    let report = analyze_path(data_path, start_time, end_time)?;
    if report.is_sweep_grade() {
        return Ok(report);
    }
    bail!("{}", blocking_error(&report));
}

/// Return a printable multiline replay-quality report.
#[must_use]
pub fn format_report(report: &ReplayQualityReport) -> String {
    let mut lines = vec![
        format!("replay_quality={}", report.class.as_str()),
        format!("start_time={}", report.start_time),
        format!("end_time={}", report.end_time),
        format!("feed_event_rows={}", report.feed_event_rows),
        format!("legacy_tick_rows={}", report.legacy_tick_rows),
    ];
    for requirement in &report.requirements {
        lines.push(format!(
            "requirement={} present={} rows={} description={}",
            requirement.key,
            requirement.present(),
            requirement.rows,
            requirement.description
        ));
    }
    lines.join("\n")
}

/// Return a concise blocking error for non-sweep-grade inputs.
#[must_use]
pub fn blocking_error(report: &ReplayQualityReport) -> String {
    let missing = report
        .missing_required()
        .into_iter()
        .map(|requirement| requirement.key)
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return format!(
            "input DB is not sweep-grade: replay_quality={}",
            report.class.as_str()
        );
    }
    format!(
        "input DB is not sweep-grade: replay_quality={} missing={}. Run `buba-paint validate-replay-data` for details.",
        report.class.as_str(),
        missing.join(",")
    )
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
    let start_time = sqlite_timestamp(start_time, "start_time")?;
    let end_time = sqlite_timestamp(end_time, "end_time")?;
    let sql = format!(
        "SELECT COUNT(*) FROM {table} WHERE {timestamp_column} >= ?1 AND {timestamp_column} <= ?2"
    );
    conn.query_row(&sql, params![start_time, end_time], |row| row.get(0))
        .with_context(|| format!("counting {table} rows"))
}

/// Build one required replay-quality row from a SQL predicate.
fn requirement(
    conn: &Connection,
    start_time: u64,
    end_time: u64,
    key: &'static str,
    description: &'static str,
    predicate: &str,
) -> anyhow::Result<ReplayQualityRequirement> {
    let start_time = sqlite_timestamp(start_time, "start_time")?;
    let end_time = sqlite_timestamp(end_time, "end_time")?;
    let sql = format!(
        "SELECT COUNT(*) FROM feed_events WHERE received_at_ms >= ?1 AND received_at_ms <= ?2 AND {predicate}"
    );
    let rows = conn
        .query_row(&sql, params![start_time, end_time], |row| row.get(0))
        .with_context(|| format!("checking replay-quality requirement: {key}"))?;
    Ok(ReplayQualityRequirement {
        key,
        description,
        rows,
        required: true,
    })
}

/// Classify one replay interval from row counts and requirements.
fn classify(
    feed_event_rows: i64,
    legacy_tick_rows: i64,
    requirements: &[ReplayQualityRequirement],
) -> ReplayQualityClass {
    if feed_event_rows > 0 && requirements.iter().all(ReplayQualityRequirement::present) {
        ReplayQualityClass::SweepGrade
    } else if feed_event_rows > 0 {
        ReplayQualityClass::DescriptiveOnly
    } else if legacy_tick_rows > 0 {
        ReplayQualityClass::LegacySnapshot
    } else {
        ReplayQualityClass::Empty
    }
}

/// Convert one replay timestamp into a SQLite-safe signed integer.
fn sqlite_timestamp(timestamp: u64, label: &str) -> anyhow::Result<i64> {
    i64::try_from(timestamp).with_context(|| format!("{label} does not fit in i64"))
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

#[cfg(test)]
#[path = "tests/replay_quality_tests.rs"]
mod tests;
