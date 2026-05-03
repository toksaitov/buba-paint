import { describe, expect, it } from "vitest";
import { redactSensitiveText } from "../logging.js";

describe("redactSensitiveText", () => {
  it("redacts SDK auth headers from JSON log fragments", () => {
    const redacted = redactSensitiveText(
      '{"headers":{"POLY_ADDRESS":"0xabc","POLY_SIGNATURE":"sig","POLY_API_KEY":"key","POLY_PASSPHRASE":"pass"}}',
    );

    expect(redacted).toContain('"POLY_SIGNATURE":"<redacted>"');
    expect(redacted).toContain('"POLY_API_KEY":"<redacted>"');
    expect(redacted).toContain('"POLY_PASSPHRASE":"<redacted>"');
    expect(redacted).not.toContain("sig");
    expect(redacted).not.toContain("key");
    expect(redacted).not.toContain("pass");
  });

  it("redacts environment-style assignments", () => {
    const redacted = redactSensitiveText(
      "POLYMARKET_PRIVATE_KEY=0xabc AGENT_SECRET=secret visible=value",
    );

    expect(redacted).toContain("POLYMARKET_PRIVATE_KEY=<redacted>");
    expect(redacted).toContain("AGENT_SECRET=<redacted>");
    expect(redacted).toContain("visible=value");
  });
});
