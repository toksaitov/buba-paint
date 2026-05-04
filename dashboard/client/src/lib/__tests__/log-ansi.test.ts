import { describe, expect, test } from "vitest";
import { parseAnsi } from "../log-ansi";

const ESC = "\x1b";

describe("parseAnsi", () => {
  test("returns plain text unchanged", () => {
    expect(parseAnsi("hello")).toEqual([{ text: "hello", className: "" }]);
  });

  test("returns empty array for empty input", () => {
    expect(parseAnsi("")).toEqual([]);
  });

  test("strips a leading reset and emits the trailing text", () => {
    expect(parseAnsi(`${ESC}[0mready`)).toEqual([
      { text: "ready", className: "" },
    ]);
  });

  test("maps red to text-accent-red and resets cleanly", () => {
    expect(parseAnsi(`${ESC}[31mERROR${ESC}[0m tail`)).toEqual([
      { text: "ERROR", className: "text-accent-red" },
      { text: " tail", className: "" },
    ]);
  });

  test("maps green to text-accent-green", () => {
    expect(parseAnsi(`${ESC}[32mok${ESC}[0m`)).toEqual([
      { text: "ok", className: "text-accent-green" },
    ]);
  });

  test("maps yellow to text-accent-blue (warning convention)", () => {
    expect(parseAnsi(`${ESC}[33mWARN${ESC}[0m`)).toEqual([
      { text: "WARN", className: "text-accent-blue" },
    ]);
  });

  test("maps blue to text-accent-blue", () => {
    expect(parseAnsi(`${ESC}[34mINFO${ESC}[0m`)).toEqual([
      { text: "INFO", className: "text-accent-blue" },
    ]);
  });

  test("maps cyan and magenta to text-muted (target metadata)", () => {
    expect(parseAnsi(`${ESC}[36mfoo${ESC}[0m`)).toEqual([
      { text: "foo", className: "text-muted" },
    ]);
    expect(parseAnsi(`${ESC}[35mbar${ESC}[0m`)).toEqual([
      { text: "bar", className: "text-muted" },
    ]);
  });

  test("maps bright red the same as red", () => {
    expect(parseAnsi(`${ESC}[91mE${ESC}[0m`)).toEqual([
      { text: "E", className: "text-accent-red" },
    ]);
  });

  test("dim overrides color and renders as muted", () => {
    expect(parseAnsi(`${ESC}[2;31mfaint${ESC}[0m`)).toEqual([
      { text: "faint", className: "text-muted" },
    ]);
  });

  test("bold adds font-semibold while preserving color", () => {
    expect(parseAnsi(`${ESC}[1;31mLOUD${ESC}[0m`)).toEqual([
      { text: "LOUD", className: "text-accent-red font-semibold" },
    ]);
  });

  test("italic adds italic class", () => {
    expect(parseAnsi(`${ESC}[3mleaning${ESC}[0m`)).toEqual([
      { text: "leaning", className: "italic" },
    ]);
  });

  test("an empty SGR (ESC[m) is treated as reset", () => {
    expect(parseAnsi(`${ESC}[31mr${ESC}[mt`)).toEqual([
      { text: "r", className: "text-accent-red" },
      { text: "t", className: "" },
    ]);
  });

  test("code 39 clears foreground but preserves bold", () => {
    expect(parseAnsi(`${ESC}[1;31mA${ESC}[39mB${ESC}[0m`)).toEqual([
      { text: "A", className: "text-accent-red font-semibold" },
      { text: "B", className: "font-semibold" },
    ]);
  });

  test("code 22 clears bold and dim but preserves color", () => {
    expect(parseAnsi(`${ESC}[1;31mA${ESC}[22mB${ESC}[0m`)).toEqual([
      { text: "A", className: "text-accent-red font-semibold" },
      { text: "B", className: "text-accent-red" },
    ]);
  });

  test("code 23 clears italic", () => {
    expect(parseAnsi(`${ESC}[3;31mA${ESC}[23mB${ESC}[0m`)).toEqual([
      { text: "A", className: "text-accent-red italic" },
      { text: "B", className: "text-accent-red" },
    ]);
  });

  test("handles multiple stretches of colored text", () => {
    expect(
      parseAnsi(`pre ${ESC}[31mred${ESC}[0m mid ${ESC}[32mgreen${ESC}[0m post`),
    ).toEqual([
      { text: "pre ", className: "" },
      { text: "red", className: "text-accent-red" },
      { text: " mid ", className: "" },
      { text: "green", className: "text-accent-green" },
      { text: " post", className: "" },
    ]);
  });

  test("ignores unknown SGR codes (e.g. background colors)", () => {
    expect(parseAnsi(`${ESC}[41;31mtext${ESC}[0m`)).toEqual([
      { text: "text", className: "text-accent-red" },
    ]);
  });

  test("does not match a bare bracket sequence without ESC", () => {
    expect(parseAnsi("[31mliteral")).toEqual([
      { text: "[31mliteral", className: "" },
    ]);
  });

  test("emits text after the last reset even without trailing reset", () => {
    expect(parseAnsi(`${ESC}[31mtail`)).toEqual([
      { text: "tail", className: "text-accent-red" },
    ]);
  });

  test("white and bright-white have no color class", () => {
    expect(parseAnsi(`${ESC}[37mw${ESC}[0m`)).toEqual([
      { text: "w", className: "" },
    ]);
    expect(parseAnsi(`${ESC}[97mW${ESC}[0m`)).toEqual([
      { text: "W", className: "" },
    ]);
  });
});
