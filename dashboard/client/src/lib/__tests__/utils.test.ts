import { describe, expect, test } from "vitest";
import {
  cn,
  formatDateTime,
  formatDurationShort,
  formatChartTick,
  formatLogTimestamp,
  formatPct,
  formatTime,
  formatUsd,
  localTimeZoneLabel,
  pnlColor,
} from "../utils";

describe("cn", () => {
  test("joins truthy classes", () => {
    expect(cn("a", "b")).toBe("a b");
  });

  test("filters falsy values", () => {
    expect(cn("a", false, null, undefined, "b")).toBe("a b");
  });

  test("returns empty string when all falsy", () => {
    expect(cn(false, null, undefined)).toBe("");
  });
});

describe("formatUsd", () => {
  test("formats positive values", () => {
    const result = formatUsd(1234.5);
    expect(result).toContain("1,234");
    expect(result).toContain("50");
    expect(result).toContain("$");
  });

  test("formats zero", () => {
    const result = formatUsd(0);
    expect(result).toContain("$");
    expect(result).toContain("0.00");
  });

  test("formats negative values", () => {
    const result = formatUsd(-50);
    expect(result).toContain("50.00");
    expect(result).toContain("-");
  });
});

describe("formatPct", () => {
  test("positive shows plus sign", () => {
    expect(formatPct(12.34)).toBe("+12.3%");
  });

  test("negative shows minus sign", () => {
    expect(formatPct(-5.67)).toBe("-5.7%");
  });

  test("zero shows plus sign", () => {
    expect(formatPct(0)).toBe("+0.0%");
  });
});

describe("formatTime", () => {
  test("renders HH:MM:SS pattern", () => {
    const result = formatTime(1767267045000);

    expect(result).toMatch(/\d{2}:\d{2}:\d{2}/);
  });
});

describe("formatDateTime", () => {
  test("renders local date, time, and zone", () => {
    const result = formatDateTime(1767267045000);

    expect(result).toMatch(/\d{4}-\d{2}-\d{2}/);
    expect(result).toMatch(/\d{2}:\d{2}:\d{2}/);
    expect(result.length).toBeGreaterThan("2026-01-01 00:00:00".length);
  });
});

describe("localTimeZoneLabel", () => {
  test("returns a string", () => {
    expect(typeof localTimeZoneLabel(1767267045000)).toBe("string");
  });
});

describe("formatLogTimestamp", () => {
  test("uses the same local timestamp policy as date time", () => {
    expect(formatLogTimestamp(1767267045000)).toBe(formatDateTime(1767267045000));
  });
});

describe("formatChartTick", () => {
  test("keeps chart axis labels compact", () => {
    expect(formatChartTick(1767267045000)).toMatch(/^\d{2}:\d{2}$/);
  });
});

describe("formatDurationShort", () => {
  test("seconds only under a minute", () => {
    expect(formatDurationShort(42_000)).toBe("42s");
  });

  test("minutes and seconds under an hour", () => {
    expect(formatDurationShort(75_000)).toBe("1m 15s");
  });

  test("hours and minutes above an hour", () => {
    expect(formatDurationShort(3_800_000)).toBe("1h 3m");
  });

  test("guards against negative and non-finite inputs", () => {
    expect(formatDurationShort(-1)).toBe("0s");
    expect(formatDurationShort(Number.NaN)).toBe("0s");
  });
});

describe("pnlColor", () => {
  test("positive returns green", () => {
    expect(pnlColor(10)).toBe("text-accent-green");
  });

  test("negative returns red", () => {
    expect(pnlColor(-10)).toBe("text-accent-red");
  });

  test("zero returns muted", () => {
    expect(pnlColor(0)).toBe("text-muted");
  });
});
