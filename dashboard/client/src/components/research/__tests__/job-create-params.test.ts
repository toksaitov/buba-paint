import { describe, expect, it } from "vitest";
import type {
  KeyValueRow,
  ResearchArtifact,
} from "../../../lib/research-types";
import {
  DEFAULT_SWEEP_ROWS,
  LARGE_INTERVAL_MS,
  artifactIntervalState,
  artifactRecency,
  buildBacktestParams,
  buildExportParams,
  customIntervalState,
  datetimeLocalFromMs,
  defaultParameterRows,
  effectiveInterval,
  initialIntervalMode,
  intervalSourceLabel,
  isoToMs,
  jobTypeName,
  mergeParams,
  parameterOption,
  parseAdditionalParams,
  resolveIntervalBoundary,
  rowsWithValues,
  sweepCombinationCount,
  sweepRowsForState,
  type BacktestState,
  type ExportState,
  type SweepState,
} from "../job-create-params";

const ARTIFACT_START_MS = Date.parse("2026-05-17T07:00");
const ARTIFACT_END_MS = Date.parse("2026-05-17T08:00");

function makeArtifact(
  overrides: Partial<ResearchArtifact> = {},
): ResearchArtifact {
  return {
    id: "art-1",
    source_machine_id: "live",
    kind: "runtime_export",
    status: "available",
    run_mode: "paper",
    artifact_root: "/r/a",
    manifest_path: "/r/a/manifest.json",
    bundle_path: null,
    source_db_path: "/r/a/paint.db",
    interval_start_ms: ARTIFACT_START_MS,
    interval_end_ms: ARTIFACT_END_MS,
    bytes: 1024,
    checksum: "deadbeef",
    replay_quality_class: "A",
    backtest_ready_class: "ready",
    live_fidelity_class: "high",
    created_at: 0,
    updated_at: 0,
    archived_at: null,
    ...overrides,
  };
}

function makeBacktestState(
  overrides: Partial<BacktestState> = {},
): BacktestState {
  return {
    artifact_id: "art-1",
    data_db_path: "",
    interval_mode: "artifact",
    start_iso: "",
    end_iso: "",
    balance: "100",
    param_source: "worker_defaults",
    confirm_interval: false,
    setOverrides: [],
    ...overrides,
  };
}

function makeExportState(overrides: Partial<ExportState> = {}): ExportState {
  return {
    source_db_path: "/x/paint.db",
    run_mode: "live_readonly",
    source_state: "stopped",
    interval_start_iso: "",
    interval_end_iso: "",
    log_paths: "",
    dry_run: true,
    confirm_export: false,
    ...overrides,
  };
}

function makeSweepState(overrides: Partial<SweepState> = {}): SweepState {
  return {
    ...makeBacktestState(),
    sweep_scope: "full",
    sweeps: DEFAULT_SWEEP_ROWS,
    ...overrides,
  };
}

describe("isoToMs", () => {
  it("returns undefined for blank input", () => {
    expect(isoToMs("")).toBeUndefined();
  });

  it("returns undefined for unparseable input", () => {
    expect(isoToMs("not-a-date")).toBeUndefined();
  });

  it("parses a datetime into milliseconds", () => {
    expect(isoToMs("2026-05-17T07:00")).toBe(Date.parse("2026-05-17T07:00"));
  });
});

describe("datetimeLocalFromMs", () => {
  it("returns empty string for null or non-finite", () => {
    expect(datetimeLocalFromMs(null)).toBe("");
    expect(datetimeLocalFromMs(undefined)).toBe("");
    expect(datetimeLocalFromMs(Number.NaN)).toBe("");
  });

  it("round-trips a datetime-local value", () => {
    const value = "2026-05-17T07:39";
    expect(datetimeLocalFromMs(Date.parse(value))).toBe(value);
  });
});

describe("resolveIntervalBoundary", () => {
  it("uses explicit input when present", () => {
    const result = resolveIntervalBoundary("2026-05-17T07:00", null);
    expect(result.source).toBe("explicit input");
    expect(result.ms).toBe(Date.parse("2026-05-17T07:00"));
  });

  it("marks invalid explicit input", () => {
    const result = resolveIntervalBoundary("garbage", 123);
    expect(result.source).toBe("invalid");
    expect(result.ms).toBeUndefined();
  });

  it("falls back to artifact milliseconds when blank", () => {
    const result = resolveIntervalBoundary("  ", 456);
    expect(result.source).toBe("artifact fallback");
    expect(result.ms).toBe(456);
  });

  it("reports missing when blank with no artifact fallback", () => {
    const result = resolveIntervalBoundary("", null);
    expect(result.source).toBe("missing");
    expect(result.ms).toBeUndefined();
  });
});

describe("effectiveInterval", () => {
  it("resolves the full artifact interval in artifact mode", () => {
    const result = effectiveInterval(makeBacktestState(), makeArtifact());
    expect(result.valid).toBe(true);
    expect(result.start.ms).toBe(ARTIFACT_START_MS);
    expect(result.end.ms).toBe(ARTIFACT_END_MS);
    expect(result.durationMs).toBe(ARTIFACT_END_MS - ARTIFACT_START_MS);
    expect(result.requiresConfirmation).toBe(false);
  });

  it("reports unusable artifact interval when bounds are missing", () => {
    const result = effectiveInterval(
      makeBacktestState(),
      makeArtifact({ interval_start_ms: null, interval_end_ms: null }),
    );
    expect(result.valid).toBe(false);
    expect(result.reason).toMatch(/does not include a usable interval/i);
  });

  it("requires custom bounds in custom mode when blank", () => {
    const result = effectiveInterval(
      makeBacktestState({ interval_mode: "custom" }),
      makeArtifact(),
    );
    expect(result.valid).toBe(false);
    expect(result.reason).toMatch(/custom start and end are required/i);
  });

  it("rejects reversed custom intervals", () => {
    const result = effectiveInterval(
      makeBacktestState({
        interval_mode: "custom",
        start_iso: "2026-05-17T09:00",
        end_iso: "2026-05-17T08:00",
      }),
      makeArtifact(),
    );
    expect(result.valid).toBe(false);
    expect(result.reason).toMatch(/end must be after start/i);
  });

  it("rejects invalid custom datetimes", () => {
    const result = effectiveInterval(
      makeBacktestState({
        interval_mode: "custom",
        start_iso: "nope",
        end_iso: "2026-05-17T08:00",
      }),
      makeArtifact(),
    );
    expect(result.valid).toBe(false);
    expect(result.reason).toMatch(/must be valid datetimes/i);
  });

  it("flags long intervals as requiring confirmation", () => {
    const start = ARTIFACT_START_MS;
    const end = start + LARGE_INTERVAL_MS + 60_000;
    const result = effectiveInterval(
      makeBacktestState({
        interval_mode: "custom",
        start_iso: datetimeLocalFromMs(start),
        end_iso: datetimeLocalFromMs(end),
      }),
      makeArtifact(),
    );
    expect(result.valid).toBe(true);
    expect(result.requiresConfirmation).toBe(true);
  });
});

describe("buildExportParams", () => {
  it("builds a dry-run payload without confirm_export", () => {
    const params = buildExportParams(makeExportState());
    expect(params).toEqual({
      source_db_path: "/x/paint.db",
      run_mode: "live_readonly",
      source_state: "stopped",
      dry_run: true,
    });
    expect(params.confirm_export).toBeUndefined();
  });

  it("includes confirm_export for real exports", () => {
    const params = buildExportParams(
      makeExportState({ dry_run: false, confirm_export: true }),
    );
    expect(params.dry_run).toBe(false);
    expect(params.confirm_export).toBe(true);
  });

  it("includes optional interval metadata and log paths when present", () => {
    const params = buildExportParams(
      makeExportState({
        interval_start_iso: "2026-05-17T07:00",
        interval_end_iso: "2026-05-17T08:00",
        log_paths: " /a/log1.log \n\n /a/log2.log \n",
      }),
    );
    expect(params.interval_start_ms).toBe(Date.parse("2026-05-17T07:00"));
    expect(params.interval_end_ms).toBe(Date.parse("2026-05-17T08:00"));
    expect(params.log_paths).toEqual(["/a/log1.log", "/a/log2.log"]);
  });

  it("omits log_paths when blank", () => {
    const params = buildExportParams(makeExportState({ log_paths: "\n  \n" }));
    expect(params.log_paths).toBeUndefined();
  });
});

describe("buildBacktestParams", () => {
  it("builds artifact-mode params with resolved interval and balance", () => {
    const params = buildBacktestParams(makeBacktestState(), makeArtifact());
    expect(params.param_source).toBe("worker_defaults");
    expect(params.interval_mode).toBe("artifact");
    expect(params.start_ms).toBe(ARTIFACT_START_MS);
    expect(params.end_ms).toBe(ARTIFACT_END_MS);
    expect(params.balance).toBe(100);
    expect(params.set).toBeUndefined();
    expect(params.data_db_path).toBeUndefined();
  });

  it("includes a trimmed data_db_path override when provided", () => {
    const params = buildBacktestParams(
      makeBacktestState({ data_db_path: "  /research/paint.db  " }),
      makeArtifact(),
    );
    expect(params.data_db_path).toBe("/research/paint.db");
  });

  it("omits non-positive or non-finite balances", () => {
    expect(
      buildBacktestParams(makeBacktestState({ balance: "0" }), makeArtifact())
        .balance,
    ).toBeUndefined();
    expect(
      buildBacktestParams(makeBacktestState({ balance: "abc" }), makeArtifact())
        .balance,
    ).toBeUndefined();
  });

  it("emits set overrides only for custom param source", () => {
    const rows: KeyValueRow[] = [
      { key: " LATENCY_ARB_MIN_ASK ", value: " 0.31 " },
      { key: "", value: "ignored" },
      { key: "MISSING_VALUE", value: "" },
    ];
    const custom = buildBacktestParams(
      makeBacktestState({ param_source: "custom", setOverrides: rows }),
      makeArtifact(),
    );
    expect(custom.set).toEqual(["LATENCY_ARB_MIN_ASK=0.31"]);

    const defaults = buildBacktestParams(
      makeBacktestState({ param_source: "worker_defaults", setOverrides: rows }),
      makeArtifact(),
    );
    expect(defaults.set).toBeUndefined();
  });
});

describe("parseAdditionalParams", () => {
  it("returns empty params for blank input", () => {
    expect(parseAdditionalParams("   ")).toEqual({ params: {}, error: null });
  });

  it("parses a JSON object", () => {
    expect(parseAdditionalParams('{"a":1}')).toEqual({
      params: { a: 1 },
      error: null,
    });
  });

  it("rejects non-object JSON", () => {
    expect(parseAdditionalParams("[1,2]").error).toMatch(
      /must be a JSON object/i,
    );
    expect(parseAdditionalParams("42").error).toMatch(/must be a JSON object/i);
    expect(parseAdditionalParams("null").error).toMatch(
      /must be a JSON object/i,
    );
  });

  it("reports a parse error for invalid JSON", () => {
    const result = parseAdditionalParams("{not json}");
    expect(result.params).toEqual({});
    expect(result.error).toBeTruthy();
  });
});

describe("mergeParams", () => {
  it("lets known params override additional params on collisions", () => {
    expect(
      mergeParams({ a: 1, b: 2 }, { b: 99, c: 3 }),
    ).toEqual({ a: 1, b: 99, c: 3 });
  });
});

describe("label helpers", () => {
  it("names job types", () => {
    expect(jobTypeName("export")).toBe("Export");
    expect(jobTypeName("sweep")).toBe("Sweep");
    expect(jobTypeName("current_params")).toBe("Backtest");
  });

  it("labels interval sources", () => {
    expect(intervalSourceLabel("explicit input")).toBe("Set by you");
    expect(intervalSourceLabel("artifact fallback")).toBe("Artifact");
    expect(intervalSourceLabel("invalid")).toBe("Invalid input");
    expect(intervalSourceLabel("missing")).toBe("Missing");
  });
});

describe("sweep helpers", () => {
  it("filters rows with both key and value", () => {
    expect(
      rowsWithValues([
        { key: "A", value: "1" },
        { key: " ", value: "2" },
        { key: "B", value: " " },
      ]),
    ).toEqual([{ key: "A", value: "1" }]);
  });

  it("returns the preset for full scope and filtered rows for focused", () => {
    expect(sweepRowsForState(makeSweepState({ sweep_scope: "full" }))).toBe(
      DEFAULT_SWEEP_ROWS,
    );
    expect(
      sweepRowsForState(
        makeSweepState({
          sweep_scope: "focused",
          sweeps: [
            { key: "A", value: "1,2" },
            { key: "", value: "" },
          ],
        }),
      ),
    ).toEqual([{ key: "A", value: "1,2" }]);
  });

  it("counts sweep combinations across comma-delimited values", () => {
    expect(sweepCombinationCount([])).toBe(0);
    expect(
      sweepCombinationCount([
        { key: "A", value: "1,2,3" },
        { key: "B", value: "x,y" },
      ]),
    ).toBe(6);
    expect(sweepCombinationCount([{ key: "A", value: "" }])).toBe(1);
  });

  it("counts the full default preset", () => {
    expect(sweepCombinationCount(DEFAULT_SWEEP_ROWS)).toBe(48);
  });
});

describe("parameter row helpers", () => {
  it("finds parameter option metadata by key", () => {
    expect(parameterOption("LATENCY_ARB_MIN_ASK")?.label).toBe(
      "Latency arb minimum ask",
    );
    expect(parameterOption("UNKNOWN")).toBeUndefined();
  });

  it("returns a default row when empty", () => {
    expect(defaultParameterRows([])).toEqual([
      { key: "LATENCY_ARB_MIN_ASK", value: "" },
    ]);
    const rows: KeyValueRow[] = [{ key: "A", value: "1" }];
    expect(defaultParameterRows(rows)).toBe(rows);
  });
});

describe("initialIntervalMode", () => {
  it("respects an explicit interval mode", () => {
    expect(initialIntervalMode({ interval_mode: "custom" })).toBe("custom");
    expect(initialIntervalMode({ interval_mode: "artifact" })).toBe("artifact");
  });

  it("infers custom when bounds are present", () => {
    expect(initialIntervalMode({ start_iso: "2026-05-17T07:00" })).toBe(
      "custom",
    );
    expect(initialIntervalMode({ end_iso: "2026-05-17T08:00" })).toBe("custom");
  });

  it("defaults to artifact mode", () => {
    expect(initialIntervalMode(undefined)).toBe("artifact");
    expect(initialIntervalMode({})).toBe("artifact");
  });
});

describe("interval state transitions", () => {
  it("resets to the artifact interval", () => {
    const next = artifactIntervalState(
      makeBacktestState({
        interval_mode: "custom",
        start_iso: "2026-05-17T07:00",
        end_iso: "2026-05-17T08:00",
        confirm_interval: true,
      }),
    );
    expect(next.interval_mode).toBe("artifact");
    expect(next.start_iso).toBe("");
    expect(next.end_iso).toBe("");
    expect(next.confirm_interval).toBe(false);
  });

  it("seeds custom bounds from the artifact when blank", () => {
    const next = customIntervalState(makeBacktestState(), makeArtifact());
    expect(next.interval_mode).toBe("custom");
    expect(next.start_iso).toBe(datetimeLocalFromMs(ARTIFACT_START_MS));
    expect(next.end_iso).toBe(datetimeLocalFromMs(ARTIFACT_END_MS));
    expect(next.confirm_interval).toBe(false);
  });

  it("keeps existing custom bounds when present", () => {
    const next = customIntervalState(
      makeBacktestState({
        start_iso: "2026-05-17T07:30",
        end_iso: "2026-05-17T07:45",
      }),
      makeArtifact(),
    );
    expect(next.start_iso).toBe("2026-05-17T07:30");
    expect(next.end_iso).toBe("2026-05-17T07:45");
  });
});

describe("artifactRecency", () => {
  it("prefers interval end, then start, then updated, then created", () => {
    expect(artifactRecency(makeArtifact())).toBe(ARTIFACT_END_MS);
    expect(
      artifactRecency(
        makeArtifact({ interval_end_ms: null, interval_start_ms: 5 }),
      ),
    ).toBe(5);
    expect(
      artifactRecency(
        makeArtifact({
          interval_end_ms: null,
          interval_start_ms: null,
          updated_at: 7,
        }),
      ),
    ).toBe(7);
    expect(
      artifactRecency(
        makeArtifact({
          interval_end_ms: null,
          interval_start_ms: null,
          updated_at: null as unknown as number,
          created_at: 9,
        }),
      ),
    ).toBe(9);
  });
});
