import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { JsonViewer } from "../json-viewer";

describe("JsonViewer", () => {
  it("renders the parse-error banner for malformed JSON and keeps the raw text", () => {
    render(<JsonViewer value="{not json" />);
    expect(screen.getByText(/could not parse json/i)).toBeInTheDocument();
    expect(
      screen.getByText((content) => content.includes("{not json")),
    ).toBeInTheDocument();
  });

  it("shows the empty label for blank string input", () => {
    render(<JsonViewer value="   " emptyLabel="Nothing here" />);
    expect(screen.getByText("Nothing here")).toBeInTheDocument();
    expect(screen.queryByText(/could not parse/i)).not.toBeInTheDocument();
  });

  it("pretty-prints valid JSON strings", () => {
    render(<JsonViewer value={'{"a":1}'} />);
    expect(screen.getByText(/"a": 1/)).toBeInTheDocument();
  });

  it("passes through non-string values without a parse error", () => {
    render(<JsonViewer value={{ a: 1 }} />);
    expect(screen.queryByText(/could not parse/i)).not.toBeInTheDocument();
    expect(screen.getByText(/"a": 1/)).toBeInTheDocument();
  });
});
