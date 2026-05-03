export type LogLevel = "info" | "warn" | "error";

const SENSITIVE_JSON_FIELD_RE =
  /"([A-Z0-9_]*(?:SIGNATURE|API_KEY|PASSPHRASE|PASSWORD|PRIVATE_KEY|SECRET|TOKEN|AUTHORIZATION|COOKIE)[A-Z0-9_]*)"\s*:\s*"[^"]*"/gi;
const SENSITIVE_ASSIGNMENT_RE =
  /\b([A-Z0-9_]*(?:SIGNATURE|API_KEY|PASSPHRASE|PASSWORD|PRIVATE_KEY|SECRET|TOKEN|AUTHORIZATION|COOKIE)[A-Z0-9_]*)=([^\s,}]+)/gi;

let consoleRedactionInstalled = false;

export function redactSensitiveText(value: string): string {
  return value
    .replace(SENSITIVE_JSON_FIELD_RE, (_match, key: string) => `"${key}":"<redacted>"`)
    .replace(SENSITIVE_ASSIGNMENT_RE, (_match, key: string) => `${key}=<redacted>`);
}

export function installConsoleRedaction(): void {
  if (consoleRedactionInstalled) {
    return;
  }
  consoleRedactionInstalled = true;
  const methods = ["log", "info", "warn", "error"] as const;
  for (const method of methods) {
    const original = console[method].bind(console);
    console[method] = (...args: unknown[]) => {
      original(
        ...args.map((arg) =>
          typeof arg === "string" ? redactSensitiveText(arg) : arg,
        ),
      );
    };
  }
}

export function logEvent(
  level: LogLevel,
  event: string,
  fields: Record<string, unknown> = {},
): void {
  const payload = {
    ts: new Date().toISOString(),
    level,
    event,
    ...fields,
  };
  process.stdout.write(`${redactSensitiveText(JSON.stringify(payload))}\n`);
}
