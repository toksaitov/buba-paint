use super::*;

// -- parse_time --

#[test]
fn parse_time_rfc3339() {
    let ms = parse_time("2026-02-20T00:00:00Z").unwrap();
    // 2026-02-20 midnight UTC
    assert_eq!(ms, 1_771_545_600_000);
}

#[test]
fn parse_time_datetime_without_seconds() {
    let ms = parse_time("2026-02-20T03:13").unwrap();
    // 2026-02-20 03:13 UTC = midnight + 3*3600 + 13*60 = 11580s
    assert_eq!(ms, 1_771_545_600_000 + 11_580_000);
}

#[test]
fn parse_time_date_only() {
    let ms = parse_time("2026-02-20").unwrap();
    assert_eq!(ms, 1_771_545_600_000);
}

#[test]
fn parse_time_with_timezone_offset() {
    let ms = parse_time("2026-02-20T00:00:00+00:00").unwrap();
    assert_eq!(ms, 1_771_545_600_000);
}

#[test]
fn parse_time_without_z_appends_z() {
    // "2026-02-20T00:00:00" has no timezone indicator.  The normalizer
    // must append Z so RFC 3339 parsing succeeds.  This previously
    // failed because ends_with("00:00") falsely matched the time.
    let with_z = parse_time("2026-02-20T00:00:00Z").unwrap();
    let without_z = parse_time("2026-02-20T00:00:00").unwrap();
    assert_eq!(with_z, without_z);
}

#[test]
fn parse_time_midnight_no_tz_works() {
    // Midnight specifically was the edge case that triggered the bug.
    let ms = parse_time("2026-02-20T00:00:00").unwrap();
    assert_eq!(ms, 1_771_545_600_000);
}

#[test]
fn parse_time_invalid_returns_error() {
    assert!(parse_time("not-a-date").is_err());
    assert!(parse_time("").is_err());
    assert!(parse_time("2026-13-40").is_err());
}

// -- apply_set_override --

#[test]
fn apply_set_override_valid() {
    let mut config = Config::default();
    apply_set_override(&mut config, "PEAK_DD_PAUSE_PCT=1.0").unwrap();
    assert!((config.peak_dd_pause_pct - 1.0).abs() < f64::EPSILON);
}

#[test]
fn apply_set_override_no_equals_returns_error() {
    let mut config = Config::default();
    assert!(apply_set_override(&mut config, "INVALID").is_err());
}

#[test]
fn apply_set_override_non_numeric_does_not_crash() {
    let mut config = Config::default();
    let original_balance = config.starting_balance;
    // Non-numeric value: prints a warning but returns Ok.
    apply_set_override(&mut config, "STARTING_BALANCE=abc").unwrap();
    // Config unchanged.
    assert!((config.starting_balance - original_balance).abs() < f64::EPSILON);
}

#[test]
fn apply_set_override_unknown_param_ok() {
    let mut config = Config::default();
    // Unknown key: set_param returns false, but no error.
    apply_set_override(&mut config, "UNKNOWN_KEY=1.0").unwrap();
}

// -- parse_key_value --

#[test]
fn parse_key_value_valid() {
    let (k, v) = parse_key_value("FOO=bar").unwrap();
    assert_eq!(k, "FOO");
    assert_eq!(v, "bar");
}

#[test]
fn parse_key_value_no_equals_returns_error() {
    assert!(parse_key_value("NOPE").is_err());
}

// -- Cli::parse_from --

#[test]
fn cli_backtest_command_parses() {
    let cli = Cli::parse_from([
        "buba-paint",
        "backtest",
        "--data",
        "test.db",
        "--start",
        "2026-02-20",
        "--end",
        "2026-02-28",
        "--balance",
        "500",
        "--set",
        "PEAK_DD_PAUSE_PCT=1.0",
    ]);
    match cli.command {
        Commands::Backtest {
            data,
            balance,
            sets,
            ..
        } => {
            assert_eq!(data, "test.db");
            assert!((balance - 500.0).abs() < f64::EPSILON);
            assert_eq!(sets.len(), 1);
            assert_eq!(sets[0], "PEAK_DD_PAUSE_PCT=1.0");
        }
        _ => panic!("expected Backtest command"),
    }
}

#[test]
fn cli_backtest_default_output() {
    let cli = Cli::parse_from([
        "buba-paint",
        "backtest",
        "--data",
        "d.db",
        "--start",
        "2026-01-01",
        "--end",
        "2026-01-02",
    ]);
    match cli.command {
        Commands::Backtest { output, .. } => {
            assert_eq!(output, "/tmp/buba-backtest.db");
        }
        _ => panic!("expected Backtest command"),
    }
}

#[test]
fn cli_sweep_command_parses() {
    let cli = Cli::parse_from([
        "buba-paint",
        "sweep",
        "--data",
        "data.db",
        "--start",
        "2026-02-20",
        "--end",
        "2026-02-28",
        "--sweep",
        "MOM=0.001:0.003:0.001",
        "--set",
        "FOO=1.0",
        "--set",
        "BAR=2.0",
    ]);
    match cli.command {
        Commands::Sweep { sweeps, sets, .. } => {
            assert_eq!(sweeps.len(), 1);
            assert_eq!(sets.len(), 2);
        }
        _ => panic!("expected Sweep command"),
    }
}

#[test]
fn cli_live_command_parses() {
    let cli = Cli::parse_from([
        "buba-paint",
        "live",
        "--db-path",
        "/tmp/test.db",
        "--balance",
        "999",
    ]);
    match cli.command {
        Commands::Live {
            db_path, balance, ..
        } => {
            assert_eq!(db_path, "/tmp/test.db");
            assert!((balance - 999.0).abs() < f64::EPSILON);
        }
        _ => panic!("expected Live command"),
    }
}

#[test]
fn parse_time_negative_utc_offset() {
    // "2026-02-20T12:00:00-05:00" should be 17:00 UTC on 2026-02-20.
    let ms = parse_time("2026-02-20T12:00:00-05:00").unwrap();
    // 2026-02-20 midnight UTC = 1_771_545_600_000
    // + 17 hours = + 17 * 3_600_000 = + 61_200_000
    let expected = 1_771_545_600_000 + 61_200_000;
    assert_eq!(ms, expected, "12:00 EST (-05:00) should be 17:00 UTC");
}

#[test]
fn parse_time_malformed_trailing_dash() {
    // "2026-02-20T00:00:00-" has a trailing dash but no offset digits.
    let result = parse_time("2026-02-20T00:00:00-");
    assert!(result.is_err(), "malformed trailing dash should return Err");
}

#[test]
fn cli_live_default_values() {
    let cli = Cli::parse_from(["buba-paint", "live"]);
    match cli.command {
        Commands::Live {
            db_path, balance, ..
        } => {
            assert_eq!(db_path, "./data/buba-paint.db");
            assert!((balance - 150.0).abs() < f64::EPSILON);
        }
        _ => panic!("expected Live command"),
    }
}
