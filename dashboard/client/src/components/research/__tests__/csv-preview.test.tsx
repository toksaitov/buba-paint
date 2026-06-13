import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { CsvPreview } from "../csv-preview";

vi.mock("../../../lib/research-api", () => ({
  downloadResearchReportCsvFromText: vi.fn(),
}));

describe("CsvPreview", () => {
  it("skips blank interior lines and counts only data rows", () => {
    render(<CsvPreview csv={"a,b\n1,2\n\n3,4\n"} />);
    expect(screen.getByText("2 rows")).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "a" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "b" })).toBeInTheDocument();
    const cells = screen.getAllByRole("cell").map((c) => c.textContent);
    expect(cells).toEqual(["1", "2", "3", "4"]);
  });

  it("treats an empty payload as a single empty header row", () => {
    render(<CsvPreview csv="" />);
    expect(screen.getByText("0 rows")).toBeInTheDocument();
    expect(screen.queryByText(/CSV payload is empty/i)).not.toBeInTheDocument();
  });

  it("truncates to maxRows and labels the truncation", () => {
    const lines = ["h"];
    for (let i = 0; i < 25; i += 1) lines.push(String(i));
    render(<CsvPreview csv={lines.join("\n")} maxRows={20} />);
    expect(screen.getByText("25 rows")).toBeInTheDocument();
    expect(screen.getByText(/showing first 20/i)).toBeInTheDocument();
  });
});
