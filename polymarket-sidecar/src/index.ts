import http from "node:http";
import { webcrypto } from "node:crypto";
import { createServer } from "./server.js";
import { loadConfig, type SidecarConfig } from "./config.js";
import {
  assertExpectedExchangeContracts,
  createDefaultProvider,
  type SidecarProvider,
} from "./provider.js";
import { installConsoleRedaction, logEvent } from "./logging.js";

export interface SidecarRuntime {
  close(): Promise<void>;
  server: http.Server;
}

interface ProcessLike {
  on(event: string, listener: (...args: unknown[]) => void): unknown;
  off?(event: string, listener: (...args: unknown[]) => void): unknown;
  once?(event: string, listener: (...args: unknown[]) => void): unknown;
  exit(code?: number): never;
}

if (!globalThis.crypto?.subtle) {
  globalThis.crypto = webcrypto as Crypto;
}

installConsoleRedaction();

export function startSidecar(
  config: SidecarConfig = loadConfig(),
  provider: SidecarProvider = createDefaultProvider(config),
  proc: ProcessLike = process,
): SidecarRuntime {
  assertExpectedExchangeContracts();
  const server = createServer(provider, config);
  let closed = false;
  let closePromise: Promise<void> | null = null;

  const close = async (): Promise<void> => {
    if (closePromise) {
      return closePromise;
    }
    if (!server.listening) {
      closePromise = Promise.resolve(provider.close()).then(() => undefined);
      return closePromise;
    }
    closePromise = new Promise<void>((resolve, reject) => {
      server.close((error) => {
        if (error) {
          reject(error);
          return;
        }
        resolve();
      });
    }).catch((error) => {
      logEvent("error", "sidecar_server_close_failed", {
        error: error instanceof Error ? error.message : String(error),
      });
      throw error;
    });
    return closePromise;
  };

  const exitAfterClose = (code: number): void => {
    if (closed) return;
    closed = true;
    void close()
      .catch(() => undefined)
      .finally(() => {
        proc.exit(code);
      });
  };

  const handleSignal = (signal: string) => {
    logEvent("info", "sidecar_shutdown_signal", { signal });
    exitAfterClose(0);
  };

  const handleFatal = (event: string, error: unknown) => {
    logEvent("error", "sidecar_fatal", {
      event,
      error: error instanceof Error ? error.message : String(error),
    });
    exitAfterClose(1);
  };

  proc.on("SIGINT", () => handleSignal("SIGINT"));
  proc.on("SIGTERM", () => handleSignal("SIGTERM"));
  proc.on("uncaughtException", (error) => {
    handleFatal("uncaughtException", error);
  });
  proc.on("unhandledRejection", (error) => {
    handleFatal("unhandledRejection", error);
  });

  server.listen(config.port, config.host, () => {
    logEvent("info", "sidecar_listening", {
      host: config.host,
      port: config.port,
    });
  });

  return {
    close,
    server,
  };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  startSidecar();
}
