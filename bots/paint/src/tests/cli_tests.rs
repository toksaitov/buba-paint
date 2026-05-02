use super::*;
use crate::config::ExecutionMode;

/// Verifies that parse time rfc3339.
#[test]
fn parse_time_rfc3339() {
    let ms = parse_time("2026-02-20T00:00:00Z").unwrap();

    assert_eq!(ms, 1_771_545_600_000);
}

/// Verifies that parse time datetime without seconds.
#[test]
fn parse_time_datetime_without_seconds() {
    let ms = parse_time("2026-02-20T03:13").unwrap();

    assert_eq!(ms, 1_771_545_600_000 + 11_580_000);
}

/// Verifies that parse time date only.
#[test]
fn parse_time_date_only() {
    let ms = parse_time("2026-02-20").unwrap();
    assert_eq!(ms, 1_771_545_600_000);
}

/// Verifies that parse time with timezone offset.
#[test]
fn parse_time_with_timezone_offset() {
    let ms = parse_time("2026-02-20T00:00:00+00:00").unwrap();
    assert_eq!(ms, 1_771_545_600_000);
}

/// Verifies that parse time without z appends z.
#[test]
fn parse_time_without_z_appends_z() {
    let with_z = parse_time("2026-02-20T00:00:00Z").unwrap();
    let without_z = parse_time("2026-02-20T00:00:00").unwrap();
    assert_eq!(with_z, without_z);
}

/// Verifies that parse time midnight no tz works.
#[test]
fn parse_time_midnight_no_tz_works() {
    let ms = parse_time("2026-02-20T00:00:00").unwrap();
    assert_eq!(ms, 1_771_545_600_000);
}

/// Verifies that parse time invalid returns error.
#[test]
fn parse_time_invalid_returns_error() {
    assert!(parse_time("not-a-date").is_err());
    assert!(parse_time("").is_err());
    assert!(parse_time("2026-13-40").is_err());
}

/// Verifies that apply set override valid.
#[test]
fn apply_set_override_valid() {
    let mut config = Config::default();
    apply_set_override(&mut config, "PEAK_DD_PAUSE_PCT=1.0").unwrap();
    assert!((config.peak_dd_pause_pct - 1.0).abs() < f64::EPSILON);
}

/// Verifies that apply set override no equals returns error.
#[test]
fn apply_set_override_no_equals_returns_error() {
    let mut config = Config::default();
    assert!(apply_set_override(&mut config, "INVALID").is_err());
}

/// Verifies that apply set override non numeric does not crash.
#[test]
fn apply_set_override_non_numeric_does_not_crash() {
    let mut config = Config::default();
    let original_balance = config.starting_balance;

    apply_set_override(&mut config, "STARTING_BALANCE=abc").unwrap();

    assert!((config.starting_balance - original_balance).abs() < f64::EPSILON);
}

/// Verifies that apply set override accepts boolean strings for known flags.
#[test]
fn apply_set_override_boolish_flag() {
    let mut config = Config::default();
    apply_set_override(&mut config, "CALM_PERSISTENCE_ENABLED=true").unwrap();
    assert!(config.calm_persistence_enabled);
    apply_set_override(&mut config, "REGIME_DETECTION_ENABLED=0").unwrap();
    assert!(!config.regime_detection_enabled);
}

/// Verifies that apply set override accepts string-backed live params.
#[test]
fn apply_set_override_string_param() {
    let mut config = Config::default();
    apply_set_override(&mut config, "EXECUTION_MODE=live_readonly").unwrap();
    apply_set_override(&mut config, "LIVE_SIDECAR_URL=http://127.0.0.1:3211").unwrap();
    assert_eq!(config.execution_mode, ExecutionMode::LiveReadonly);
    assert_eq!(config.live_sidecar_url, "http://127.0.0.1:3211");
}

/// Verifies that apply set override unknown param ok.
#[test]
fn apply_set_override_unknown_param_ok() {
    let mut config = Config::default();

    apply_set_override(&mut config, "UNKNOWN_KEY=1.0").unwrap();
}

/// Verifies that parse key value valid.
#[test]
fn parse_key_value_valid() {
    let (k, v) = parse_key_value("FOO=bar").unwrap();
    assert_eq!(k, "FOO");
    assert_eq!(v, "bar");
}

/// Verifies that parse key value no equals returns error.
#[test]
fn parse_key_value_no_equals_returns_error() {
    assert!(parse_key_value("NOPE").is_err());
}

/// Verifies that cli backtest command parses.
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

/// Verifies that cli backtest default output.
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

/// Verifies that cli sweep command parses.
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

/// Verifies that cli validate replay data command parses.
#[test]
fn cli_validate_replay_data_command_parses() {
    let cli = Cli::parse_from([
        "buba-paint",
        "validate-replay-data",
        "--data",
        "data.db",
        "--start",
        "2026-02-20",
        "--end",
        "2026-02-28",
    ]);
    match cli.command {
        Commands::ValidateReplayData { data, .. } => {
            assert_eq!(data, "data.db");
        }
        _ => panic!("expected ValidateReplayData command"),
    }
}

/// Verifies that cli live command parses.
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

/// Verifies that cli live-preflight command parses.
#[test]
fn cli_live_preflight_command_parses() {
    let cli = Cli::parse_from([
        "buba-paint",
        "live-preflight",
        "--set",
        "EXECUTION_MODE=live_readonly",
        "--set",
        "LIVE_SESSION_CASH_CAP_USD=75",
    ]);
    match cli.command {
        Commands::LivePreflight { sets } => {
            assert_eq!(
                sets,
                vec![
                    "EXECUTION_MODE=live_readonly".to_string(),
                    "LIVE_SESSION_CASH_CAP_USD=75".to_string(),
                ]
            );
        }
        _ => panic!("expected LivePreflight command"),
    }
}

/// Verifies that cli validate-live-fidelity command parses optional output.
#[test]
fn cli_validate_live_fidelity_command_parses() {
    let cli = Cli::parse_from([
        "buba-paint",
        "validate-live-fidelity",
        "--db-path",
        "/tmp/live.db",
        "--start",
        "2026-05-01T00:00:00Z",
        "--end",
        "2026-05-01T01:00:00Z",
        "--output",
        "/tmp/live-fidelity.json",
    ]);
    match cli.command {
        Commands::ValidateLiveFidelity {
            db_path,
            start,
            end,
            output,
        } => {
            assert_eq!(db_path, "/tmp/live.db");
            assert_eq!(start, "2026-05-01T00:00:00Z");
            assert_eq!(end, "2026-05-01T01:00:00Z");
            assert_eq!(output.as_deref(), Some("/tmp/live-fidelity.json"));
        }
        _ => panic!("expected ValidateLiveFidelity command"),
    }
}

/// Verifies that cli live-control command parses durable operator fields.
#[test]
fn cli_live_control_command_parses() {
    let cli = Cli::parse_from([
        "buba-paint",
        "live-control",
        "--db-path",
        "/tmp/live.db",
        "arm",
        "--actor",
        "operator",
        "--reason",
        "preflight passed",
    ]);
    match cli.command {
        Commands::LiveControl {
            db_path,
            action,
            actor,
            reason,
        } => {
            assert_eq!(db_path, "/tmp/live.db");
            assert_eq!(action, "arm");
            assert_eq!(actor, "operator");
            assert_eq!(reason, "preflight passed");
        }
        _ => panic!("expected LiveControl command"),
    }
}

/// Verifies that cli live-closeout command parses required evidence fields.
#[test]
fn cli_live_closeout_command_parses() {
    let cli = Cli::parse_from([
        "buba-paint",
        "live-closeout",
        "--db-path",
        "/tmp/live.db",
        "--output-dir",
        "/tmp/closeout",
        "--actor",
        "operator",
        "--reason",
        "terminal halt",
    ]);
    match cli.command {
        Commands::LiveCloseout {
            db_path,
            output_dir,
            actor,
            reason,
        } => {
            assert_eq!(db_path, "/tmp/live.db");
            assert_eq!(output_dir, "/tmp/closeout");
            assert_eq!(actor, "operator");
            assert_eq!(reason, "terminal halt");
        }
        _ => panic!("expected LiveCloseout command"),
    }
}

/// Verifies that cli db footprint command parses.
#[test]
fn cli_db_footprint_command_parses() {
    let cli = Cli::parse_from(["buba-paint", "db-footprint", "--db-path", "/tmp/test.db"]);
    match cli.command {
        Commands::DbFootprint { db_path } => {
            assert_eq!(db_path, "/tmp/test.db");
        }
        _ => panic!("expected DbFootprint command"),
    }
}

/// Verifies that parse time negative utc offset.
#[test]
fn parse_time_negative_utc_offset() {
    let ms = parse_time("2026-02-20T12:00:00-05:00").unwrap();

    let expected = 1_771_545_600_000 + 61_200_000;
    assert_eq!(ms, expected, "12:00 EST (-05:00) should be 17:00 UTC");
}

/// Verifies that parse time malformed trailing dash.
#[test]
fn parse_time_malformed_trailing_dash() {
    let result = parse_time("2026-02-20T00:00:00-");
    assert!(result.is_err(), "malformed trailing dash should return Err");
}

/// Verifies that apply set override non numeric value.
#[test]
fn apply_set_override_non_numeric_value() {
    let mut config = Config::default();
    let original_momentum = config.latency_arb_momentum_threshold;
    let result = apply_set_override(&mut config, "LATENCY_ARB_MOMENTUM_THRESHOLD=notanumber");
    assert!(result.is_ok(), "non-numeric value should not return Err");
    assert!(
        (config.latency_arb_momentum_threshold - original_momentum).abs() < f64::EPSILON,
        "config value should be unchanged after non-numeric override"
    );
}

/// Verifies that parse key value missing equals.
#[test]
fn parse_key_value_missing_equals() {
    let result = parse_key_value("noequals");
    assert!(
        result.is_err(),
        "parse_key_value with no '=' should return Err"
    );
}

/// Verifies that cli build data subcommand parses.
#[test]
fn cli_build_data_subcommand_parses() {
    let cli = Cli::parse_from([
        "buba-paint",
        "build-data",
        "--runs-dir",
        "my_runs",
        "--output",
        "my_output.db",
    ]);
    match cli.command {
        Commands::BuildData { runs_dir, output } => {
            assert_eq!(runs_dir, "my_runs");
            assert_eq!(output, "my_output.db");
        }
        _ => panic!("expected BuildData command"),
    }
}

/// Verifies that cli live default values.
#[test]
fn cli_live_default_values() {
    let cli = Cli::parse_from(["buba-paint", "live"]);
    match cli.command {
        Commands::Live {
            db_path, balance, ..
        } => {
            assert_eq!(db_path, "./data/paint.db");
            assert!((balance - 150.0).abs() < f64::EPSILON);
        }
        _ => panic!("expected Live command"),
    }
}

/// Verifies that cli latency probe parses timeout and overrides.
#[test]
fn cli_latency_probe_parses() {
    let cli = Cli::parse_from([
        "buba-paint",
        "latency-probe",
        "--timeout-ms",
        "7500",
        "--set",
        "MAX_QUOTE_AGE_MS=350",
    ]);
    match cli.command {
        Commands::LatencyProbe { timeout_ms, sets } => {
            assert_eq!(timeout_ms, 7500);
            assert_eq!(sets, vec!["MAX_QUOTE_AGE_MS=350"]);
        }
        _ => panic!("expected LatencyProbe command"),
    }
}
