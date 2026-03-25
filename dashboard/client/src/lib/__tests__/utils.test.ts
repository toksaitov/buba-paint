import { describe, expect, test } from "vitest";
import { cn, formatUsd, formatPct, formatTime, formatDateTime, pnlColor } from "../utils";

// -- cn --

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

// -- formatUsd --

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

// -- formatPct --

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

// -- formatTime --

describe("formatTime", () => {
  test("renders HH:MM:SS pattern", () => {
    // 2026-01-01T12:30:45Z = 1767267045000
    const result = formatTime(1767267045000);
    // Must contain "30" and "45" for minutes and seconds.
    expect(result).toMatch(/\d{2}:\d{2}:\d{2}/);
  });
});

// -- formatDateTime --

describe("formatDateTime", () => {
  test("renders abbreviated date and time", () => {
    const result = formatDateTime(1767267045000);
    // Should contain a month abbreviation and time.
    expect(result).toMatch(/\w{3}/);
    expect(result).toMatch(/\d{2}:\d{2}:\d{2}/);
  });
});

// -- pnlColor --

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
