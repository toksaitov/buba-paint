import { CONFIG } from "../config.js";

const LEVELS = { debug: 0, info: 1, warn: 2, error: 3 } as const;

type Level = keyof typeof LEVELS;

function shouldLog(level: Level): boolean {
  return LEVELS[level] >= LEVELS[CONFIG.LOG_LEVEL];
}

function serialize(data: unknown): string {
  if (typeof data === "string") return data;
  if (data instanceof Error) return data.stack ?? data.message;
  return JSON.stringify(data);
}

function fmt(level: Level, module: string, msg: string, data?: unknown): string {
  const ts = new Date().toISOString();
  const base = `[${ts}] [${level.toUpperCase().padEnd(5)}] [${module}] ${msg}`;
  if (data !== undefined) {
    return `${base} ${serialize(data)}`;
  }
  return base;
}

export interface Logger {
  debug(msg: string, data?: unknown): void;
  info(msg: string, data?: unknown): void;
  warn(msg: string, data?: unknown): void;
  error(msg: string, data?: unknown): void;
}

export function createLogger(module: string): Logger {
  return {
    debug(msg, data?) {
      if (shouldLog("debug")) console.debug(fmt("debug", module, msg, data));
    },
    info(msg, data?) {
      if (shouldLog("info")) console.log(fmt("info", module, msg, data));
    },
    warn(msg, data?) {
      if (shouldLog("warn")) console.warn(fmt("warn", module, msg, data));
    },
    error(msg, data?) {
      if (shouldLog("error")) console.error(fmt("error", module, msg, data));
    },
  };
}
