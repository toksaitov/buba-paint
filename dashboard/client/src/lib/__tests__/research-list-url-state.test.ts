import { describe, expect, it } from "vitest";
import {
  readEnumListParam,
  readEnumParam,
  readTextParam,
  sameEnumSet,
  setQueryListParam,
  setQueryParam,
} from "../research-list-url-state";

const STATUSES = ["queued", "running", "completed"] as const;

describe("readEnumParam", () => {
  it("returns the value when allowed and falls back otherwise", () => {
    const params = new URLSearchParams("status=running&bad=nope");
    expect(readEnumParam(params, "status", STATUSES, "queued")).toBe("running");
    expect(readEnumParam(params, "bad", STATUSES, "queued")).toBe("queued");
    expect(readEnumParam(params, "missing", STATUSES, "queued")).toBe("queued");
  });
});

describe("readTextParam", () => {
  it("returns the raw value or an empty string", () => {
    const params = new URLSearchParams("q=abc");
    expect(readTextParam(params, "q")).toBe("abc");
    expect(readTextParam(params, "missing")).toBe("");
  });
});

describe("readEnumListParam", () => {
  it("returns the fallback when the param is absent", () => {
    expect(
      readEnumListParam(new URLSearchParams(), "s", STATUSES, ["queued"]),
    ).toEqual(["queued"]);
  });
  it("treats the literal none as an empty selection", () => {
    expect(
      readEnumListParam(new URLSearchParams("s=none"), "s", STATUSES, [
        "queued",
      ]),
    ).toEqual([]);
  });
  it("parses a comma list and drops unknown entries", () => {
    expect(
      readEnumListParam(
        new URLSearchParams("s=running, completed ,bogus"),
        "s",
        STATUSES,
        ["queued"],
      ),
    ).toEqual(["running", "completed"]);
  });
  it("falls back when every entry is malformed", () => {
    expect(
      readEnumListParam(new URLSearchParams("s=bogus,nope"), "s", STATUSES, [
        "queued",
      ]),
    ).toEqual(["queued"]);
  });
});

describe("setQueryParam round-trip", () => {
  it("deletes on fallback or empty and sets otherwise", () => {
    const params = new URLSearchParams("status=running");
    setQueryParam(params, "status", "queued", "queued");
    expect(params.has("status")).toBe(false);
    setQueryParam(params, "status", "completed", "queued");
    expect(readEnumParam(params, "status", STATUSES, "queued")).toBe(
      "completed",
    );
    setQueryParam(params, "status", "", "queued");
    expect(params.has("status")).toBe(false);
  });
});

describe("setQueryListParam round-trip", () => {
  it("serializes to none for empty and to a comma list otherwise", () => {
    const fallback = ["queued", "running", "completed"] as const;
    const params = new URLSearchParams();
    setQueryListParam(params, "s", [], fallback);
    expect(params.get("s")).toBe("none");
    expect(readEnumListParam(params, "s", STATUSES, fallback)).toEqual([]);
    setQueryListParam(params, "s", ["running", "completed"], fallback);
    expect(params.get("s")).toBe("running,completed");
    expect(readEnumListParam(params, "s", STATUSES, fallback)).toEqual([
      "running",
      "completed",
    ]);
  });
  it("deletes the param when the selection equals the fallback", () => {
    const fallback = ["queued", "running"] as const;
    const params = new URLSearchParams("s=running,queued");
    setQueryListParam(params, "s", ["queued", "running"], fallback);
    expect(params.has("s")).toBe(false);
  });
});

describe("sameEnumSet", () => {
  it("ignores order and detects membership differences", () => {
    expect(sameEnumSet(["a", "b"], ["b", "a"])).toBe(true);
    expect(sameEnumSet(["a"], ["a", "b"])).toBe(false);
  });
});
