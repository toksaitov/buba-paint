use super::*;

/// Verifies that parse range three values.
#[test]
fn parse_range_three_values() {
    let dim = parse_sweep_spec("PARAM=0.001:0.003:0.001").unwrap();
    assert_eq!(dim.param, "PARAM");
    assert_eq!(dim.values.len(), 3);
    assert!((dim.values[0] - 0.001).abs() < 1e-9);
    assert!((dim.values[1] - 0.002).abs() < 1e-9);
    assert!((dim.values[2] - 0.003).abs() < 1e-9);
}

/// Verifies that parse range single value.
#[test]
fn parse_range_single_value() {
    let dim = parse_sweep_spec("X=5.0:5.0:1.0").unwrap();
    assert_eq!(dim.values.len(), 1);
    assert!((dim.values[0] - 5.0).abs() < f64::EPSILON);
}

/// Verifies that parse range integer step.
#[test]
fn parse_range_integer_step() {
    let dim = parse_sweep_spec("N=10:30:10").unwrap();
    assert_eq!(dim.values.len(), 3);
    assert!((dim.values[0] - 10.0).abs() < 1e-9);
    assert!((dim.values[1] - 20.0).abs() < 1e-9);
    assert!((dim.values[2] - 30.0).abs() < 1e-9);
}

/// Verifies that parse comma list three values.
#[test]
fn parse_comma_list_three_values() {
    let dim = parse_sweep_spec("PARAM=0.45,0.50,0.55").unwrap();
    assert_eq!(dim.param, "PARAM");
    assert_eq!(dim.values.len(), 3);
    assert!((dim.values[0] - 0.45).abs() < f64::EPSILON);
    assert!((dim.values[1] - 0.50).abs() < f64::EPSILON);
    assert!((dim.values[2] - 0.55).abs() < f64::EPSILON);
}

/// Verifies that parse comma list two values.
#[test]
fn parse_comma_list_two_values() {
    let dim = parse_sweep_spec("X=1.0,2.0").unwrap();
    assert_eq!(dim.param, "X");
    assert_eq!(dim.values.len(), 2);
    assert!((dim.values[0] - 1.0).abs() < f64::EPSILON);
    assert!((dim.values[1] - 2.0).abs() < f64::EPSILON);
}

/// Verifies that parse no equals returns error.
#[test]
fn parse_no_equals_returns_error() {
    let result = parse_sweep_spec("PARAM_0.001:0.003:0.001");
    assert!(result.is_err());
}

/// Verifies that parse invalid range two parts.
#[test]
fn parse_invalid_range_two_parts() {
    let result = parse_sweep_spec("PARAM=0.001:0.003");
    assert!(result.is_err());
}

/// Verifies that parse invalid range one part no comma.
#[test]
fn parse_invalid_range_one_part_no_comma() {
    let result = parse_sweep_spec("PARAM=42.0");
    assert!(result.is_err());
}

/// Verifies that parse zero step returns error.
#[test]
fn parse_zero_step_returns_error() {
    let result = parse_sweep_spec("PARAM=1.0:3.0:0.0");
    assert!(result.is_err());
}

/// Verifies that parse negative step returns error.
#[test]
fn parse_negative_step_returns_error() {
    let result = parse_sweep_spec("PARAM=3.0:1.0:-1.0");
    assert!(result.is_err());
}

/// Verifies that parse comma list invalid number.
#[test]
fn parse_comma_list_invalid_number() {
    let result = parse_sweep_spec("PARAM=0.45,abc,0.55");
    assert!(result.is_err());
}

/// Verifies that cartesian empty dims.
#[test]
fn cartesian_empty_dims() {
    let result = cartesian(&[]);
    assert_eq!(result.len(), 1);
    assert!(result[0].is_empty());
}

/// Verifies that cartesian single dim.
#[test]
fn cartesian_single_dim() {
    let dims = vec![SweepDimension {
        param: "A".to_string(),
        values: vec![1.0, 2.0, 3.0],
    }];
    let result = cartesian(&dims);
    assert_eq!(result.len(), 3);
    assert!((result[0][0].1 - 1.0).abs() < f64::EPSILON);
    assert!((result[1][0].1 - 2.0).abs() < f64::EPSILON);
    assert!((result[2][0].1 - 3.0).abs() < f64::EPSILON);
}

/// Verifies that cartesian two dims.
#[test]
fn cartesian_two_dims() {
    let dims = vec![
        SweepDimension {
            param: "A".to_string(),
            values: vec![1.0, 2.0],
        },
        SweepDimension {
            param: "B".to_string(),
            values: vec![10.0, 20.0, 30.0],
        },
    ];
    let result = cartesian(&dims);
    assert_eq!(result.len(), 6);

    assert_eq!(result[0][0].0, "A");
    assert_eq!(result[0][1].0, "B");
}

/// Verifies that cartesian three dims.
#[test]
fn cartesian_three_dims() {
    let dims = vec![
        SweepDimension {
            param: "X".to_string(),
            values: vec![1.0, 2.0],
        },
        SweepDimension {
            param: "Y".to_string(),
            values: vec![3.0, 4.0],
        },
        SweepDimension {
            param: "Z".to_string(),
            values: vec![5.0],
        },
    ];
    let result = cartesian(&dims);
    assert_eq!(result.len(), 4);

    for combo in &result {
        assert_eq!(combo.len(), 3);
    }
}

/// Verifies that cartesian preserves param names.
#[test]
fn cartesian_preserves_param_names() {
    let dims = vec![
        SweepDimension {
            param: "MOMENTUM".to_string(),
            values: vec![0.001],
        },
        SweepDimension {
            param: "MAX_ASK".to_string(),
            values: vec![0.55],
        },
    ];
    let result = cartesian(&dims);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0][0].0, "MOMENTUM");
    assert_eq!(result[0][1].0, "MAX_ASK");
}

/// Helper: build a `BacktestResult` with the given `PnL` (other fields zeroed).
fn result_with_pnl(pnl: f64) -> BacktestResult {
    BacktestResult {
        start_time: 0,
        end_time: 0,
        duration_hours: 0.0,
        elapsed_seconds: 1.5,
        total_ticks: 100,
        total_windows: 1,
        signals: 5,
        trades: 3,
        wins: 2,
        losses: 1,
        win_rate: 0.667,
        final_balance: 200.0 + pnl,
        total_pnl: pnl,
        gross_pnl: pnl,
        max_drawdown_pct: 0.05,
        high_water_mark: 200.0 + pnl,
        total_fees: 0.0,
        pnl_net: pnl,
        fill_rate: 1.0,
        partial_fill_rate: 0.0,
        no_fill_count: 0,
        spread_legging_count: 0,
        residual_position_count: 0,
        avg_fill_latency_ms: 250.0,
        avg_slippage: 0.0,
        raw_event_batches: 0,
        legacy_snapshot_batches: 1,
    }
}

/// Verifies that build csv empty results just header.
#[test]
fn build_csv_empty_results_just_header() {
    let dim_names = vec!["MOMENTUM".to_string(), "MAX_ASK".to_string()];
    let results: Vec<(Vec<(&str, f64)>, BacktestResult)> = vec![];
    let csv = build_csv(&dim_names, &results);

    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 1, "empty results should produce only a header");
    assert_eq!(
        lines[0],
        "MOMENTUM,MAX_ASK,pnl,win_rate,trades,wins,losses,max_dd,hwm,final_balance,signals,fill_rate,partial_fill_rate,no_fill_count,spread_legging_count,residual_position_count,avg_fill_latency_ms,avg_slippage,raw_event_batches,legacy_snapshot_batches,total_fees,gross_pnl,pnl_net,elapsed_s"
    );
}

/// Verifies that build csv single result correct row.
#[test]
fn build_csv_single_result_correct_row() {
    let dim_names = vec!["PARAM_A".to_string()];
    let results: Vec<(Vec<(&str, f64)>, BacktestResult)> =
        vec![(vec![("PARAM_A", 0.5)], result_with_pnl(42.0))];
    let csv = build_csv(&dim_names, &results);

    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 2, "should have header + 1 data row");

    assert_eq!(
        lines[0],
        "PARAM_A,pnl,win_rate,trades,wins,losses,max_dd,hwm,final_balance,signals,fill_rate,partial_fill_rate,no_fill_count,spread_legging_count,residual_position_count,avg_fill_latency_ms,avg_slippage,raw_event_batches,legacy_snapshot_batches,total_fees,gross_pnl,pnl_net,elapsed_s"
    );

    assert!(
        lines[1].starts_with("0.5,"),
        "row should start with param value: {}",
        lines[1]
    );

    let cols: Vec<&str> = lines[1].split(',').collect();
    assert_eq!(cols.len(), 23, "should have 1 param + 22 metric columns");
    assert_eq!(cols[0], "0.5");
    assert_eq!(cols[1], "42");
    assert_eq!(cols[3], "3");
    assert_eq!(cols[4], "2");
    assert_eq!(cols[5], "1");
}

/// Verifies that build csv header matches expected columns.
#[test]
fn build_csv_header_matches_expected_columns() {
    let dim_names = vec!["X".to_string(), "Y".to_string(), "Z".to_string()];
    let results: Vec<(Vec<(&str, f64)>, BacktestResult)> = vec![];
    let csv = build_csv(&dim_names, &results);

    let header = csv.lines().next().unwrap();
    let cols: Vec<&str> = header.split(',').collect();

    assert_eq!(cols.len(), 25);
    assert_eq!(cols[0], "X");
    assert_eq!(cols[1], "Y");
    assert_eq!(cols[2], "Z");
    assert_eq!(cols[3], "pnl");
    assert_eq!(cols[4], "win_rate");
    assert_eq!(cols[5], "trades");
    assert_eq!(cols[6], "wins");
    assert_eq!(cols[7], "losses");
    assert_eq!(cols[8], "max_dd");
    assert_eq!(cols[9], "hwm");
    assert_eq!(cols[10], "final_balance");
    assert_eq!(cols[11], "signals");
    assert_eq!(cols[12], "fill_rate");
    assert_eq!(cols[13], "partial_fill_rate");
    assert_eq!(cols[14], "no_fill_count");
    assert_eq!(cols[15], "spread_legging_count");
    assert_eq!(cols[16], "residual_position_count");
    assert_eq!(cols[17], "avg_fill_latency_ms");
    assert_eq!(cols[18], "avg_slippage");
    assert_eq!(cols[19], "raw_event_batches");
    assert_eq!(cols[20], "legacy_snapshot_batches");
    assert_eq!(cols[21], "total_fees");
    assert_eq!(cols[22], "gross_pnl");
    assert_eq!(cols[23], "pnl_net");
    assert_eq!(cols[24], "elapsed_s");
}

/// Verifies that build csv no dims only metrics.
#[test]
fn build_csv_no_dims_only_metrics() {
    let dim_names: Vec<String> = vec![];
    let results: Vec<(Vec<(&str, f64)>, BacktestResult)> = vec![(vec![], result_with_pnl(10.0))];
    let csv = build_csv(&dim_names, &results);

    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 2);

    let header_cols: Vec<&str> = lines[0].split(',').collect();
    assert_eq!(header_cols.len(), 22);
    assert_eq!(header_cols[0], "pnl");
}

/// Verifies that build csv multiple results preserves order.
#[test]
fn build_csv_multiple_results_preserves_order() {
    let dim_names = vec!["P".to_string()];
    let results: Vec<(Vec<(&str, f64)>, BacktestResult)> = vec![
        (vec![("P", 1.0)], result_with_pnl(100.0)),
        (vec![("P", 2.0)], result_with_pnl(50.0)),
        (vec![("P", 3.0)], result_with_pnl(200.0)),
    ];
    let csv = build_csv(&dim_names, &results);

    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 4);

    assert!(lines[1].starts_with("1,"));
    assert!(lines[2].starts_with("2,"));
    assert!(lines[3].starts_with("3,"));
}

/// Verifies that top n by pnl three results top two.
#[test]
fn top_n_by_pnl_three_results_top_two() {
    let results: Vec<(Vec<(&str, f64)>, BacktestResult)> = vec![
        (vec![("P", 1.0)], result_with_pnl(50.0)),
        (vec![("P", 2.0)], result_with_pnl(200.0)),
        (vec![("P", 3.0)], result_with_pnl(100.0)),
    ];
    let top = top_n_by_pnl(&results, 2);

    assert_eq!(top.len(), 2);

    assert!((top[0].1.total_pnl - 200.0).abs() < f64::EPSILON);
    assert!((top[1].1.total_pnl - 100.0).abs() < f64::EPSILON);
}

/// Verifies that top n by pnl fewer results than n.
#[test]
fn top_n_by_pnl_fewer_results_than_n() {
    let results: Vec<(Vec<(&str, f64)>, BacktestResult)> =
        vec![(vec![("P", 1.0)], result_with_pnl(50.0))];
    let top = top_n_by_pnl(&results, 5);

    assert_eq!(top.len(), 1, "should return all when fewer than N");
    assert!((top[0].1.total_pnl - 50.0).abs() < f64::EPSILON);
}

/// Verifies that top n by pnl empty results.
#[test]
fn top_n_by_pnl_empty_results() {
    let results: Vec<(Vec<(&str, f64)>, BacktestResult)> = vec![];
    let top = top_n_by_pnl(&results, 3);
    assert!(top.is_empty());
}

/// Verifies that top n by pnl equal pnl stable order.
#[test]
fn top_n_by_pnl_equal_pnl_stable_order() {
    let results: Vec<(Vec<(&str, f64)>, BacktestResult)> = vec![
        (vec![("P", 1.0)], result_with_pnl(100.0)),
        (vec![("P", 2.0)], result_with_pnl(100.0)),
        (vec![("P", 3.0)], result_with_pnl(100.0)),
    ];
    let top = top_n_by_pnl(&results, 2);

    assert_eq!(top.len(), 2);

    assert!(
        (top[0].0[0].1 - 1.0).abs() < f64::EPSILON,
        "first should be P=1.0"
    );
    assert!(
        (top[1].0[0].1 - 2.0).abs() < f64::EPSILON,
        "second should be P=2.0"
    );
}

/// Verifies that top n by pnl negative pnl sorted correctly.
#[test]
fn top_n_by_pnl_negative_pnl_sorted_correctly() {
    let results: Vec<(Vec<(&str, f64)>, BacktestResult)> = vec![
        (vec![("P", 1.0)], result_with_pnl(-10.0)),
        (vec![("P", 2.0)], result_with_pnl(-50.0)),
        (vec![("P", 3.0)], result_with_pnl(5.0)),
    ];
    let top = top_n_by_pnl(&results, 3);

    assert_eq!(top.len(), 3);
    assert!((top[0].1.total_pnl - 5.0).abs() < f64::EPSILON);
    assert!((top[1].1.total_pnl - (-10.0)).abs() < f64::EPSILON);
    assert!((top[2].1.total_pnl - (-50.0)).abs() < f64::EPSILON);
}

/// Verifies that parse sweep spec zero step.
#[test]
fn parse_sweep_spec_zero_step() {
    let result = parse_sweep_spec("PARAM=0.001:0.003:0.0");
    assert!(
        result.is_err(),
        "zero step should return Err to prevent infinite loop"
    );
}

/// Verifies that parse sweep spec reversed range.
#[test]
fn parse_sweep_spec_reversed_range() {
    let result = parse_sweep_spec("PARAM=0.003:0.001:0.001");
    assert!(result.is_ok(), "reversed range should not error");
    let dim = result.unwrap();
    assert_eq!(
        dim.values.len(),
        0,
        "reversed range (start > end) should produce 0 values"
    );
}

/// Verifies that top n by pnl n zero returns empty.
#[test]
fn top_n_by_pnl_n_zero_returns_empty() {
    let results: Vec<(Vec<(&str, f64)>, BacktestResult)> =
        vec![(vec![("P", 1.0)], result_with_pnl(100.0))];
    let top = top_n_by_pnl(&results, 0);
    assert!(top.is_empty(), "N=0 should return empty");
}
