#!/usr/bin/env node
// Independent Polymarket CLOB V2 auth probe.
// Implements L1 (EIP-712) and L2 (HMAC-SHA256) signing inline using only
// @ethersproject/wallet plus node:https. Does not import sidecar source.
//
// Reads credentials from process.env. Caller is expected to source the env
// file before invoking.
//
// Usage:
//   node scripts/clob-auth-probe.mjs --out /tmp/clob-probe.json [--iterations 20]
//                                    [--nonce-scan 0:10] [--no-create]
//                                    [--signature-types 1,2]
//
// Output: JSON evidence file (no secret values), stdout summary (redacted).

import https from "node:https";
import crypto from "node:crypto";
import fs from "node:fs/promises";
import path from "node:path";
import { Wallet } from "@ethersproject/wallet";

const POLYGON_CHAIN_ID = 137;
const MSG_TO_SIGN = "This message attests that I control the given wallet";
const USER_AGENT = "@polymarket/clob-client";
const DEFAULT_HOST = "https://clob.polymarket.com";

function parseArgs(argv) {
  const args = {
    out: null,
    iterations: 25,
    nonceScan: [0, 1, 2, 3, 4, 5],
    tryCreate: true,
    forceCreate: false,
    signatureTypes: null,
  };
  for (let i = 2; i < argv.length; i += 1) {
    const a = argv[i];
    if (a === "--out") args.out = argv[++i];
    else if (a.startsWith("--out=")) args.out = a.slice(6);
    else if (a === "--iterations") args.iterations = Number.parseInt(argv[++i], 10);
    else if (a.startsWith("--iterations=")) args.iterations = Number.parseInt(a.slice(13), 10);
    else if (a === "--nonce-scan") args.nonceScan = parseRange(argv[++i]);
    else if (a.startsWith("--nonce-scan=")) args.nonceScan = parseRange(a.slice(13));
    else if (a === "--no-create") args.tryCreate = false;
    else if (a === "--force-create") args.forceCreate = true;
    else if (a === "--signature-types") args.signatureTypes = parseList(argv[++i]);
    else if (a.startsWith("--signature-types=")) args.signatureTypes = parseList(a.slice(18));
    else throw new Error(`unknown argument: ${a}`);
  }
  if (!args.out) throw new Error("--out is required");
  if (!Number.isFinite(args.iterations) || args.iterations < 1) {
    throw new Error("--iterations must be a positive integer");
  }
  return args;
}

function parseRange(value) {
  if (!value) return [];
  if (value.includes(":")) {
    const [lo, hi] = value.split(":").map((p) => Number.parseInt(p, 10));
    if (!Number.isFinite(lo) || !Number.isFinite(hi) || hi < lo) {
      throw new Error(`invalid --nonce-scan range: ${value}`);
    }
    const out = [];
    for (let n = lo; n <= hi; n += 1) out.push(n);
    return out;
  }
  return value.split(",").map((p) => {
    const n = Number.parseInt(p.trim(), 10);
    if (!Number.isFinite(n)) throw new Error(`invalid nonce: ${p}`);
    return n;
  });
}

function parseList(value) {
  if (!value) return null;
  return value.split(",").map((p) => {
    const n = Number.parseInt(p.trim(), 10);
    if (!Number.isFinite(n)) throw new Error(`invalid signature type: ${p}`);
    return n;
  });
}

function utcNow() {
  return new Date().toISOString().replace(/\.\d{3}Z$/, "Z");
}

function nowMs() {
  return Date.now();
}

function compactBody(body, max = 240) {
  return body.replace(/\s+/g, " ").slice(0, max);
}

function redactSecretsInString(value) {
  if (!value) return value;
  return value
    .replace(/("?(?:secret|passphrase|key|api_key|signature|private[-_]?key|cookie|authorization)"?\s*[:=]\s*)("[^"]*"|[^\s,;]+)/gi, "$1<redacted>")
    .replace(/0x[0-9a-fA-F]{30,}/g, "0x<redacted-long-hex>");
}

const REDACT_KEY_PATTERNS = [
  /^private[_-]?key$/i,
  /^secret$/i,
  /^api[_-]?secret$/i,
  /^passphrase$/i,
  /^api[_-]?passphrase$/i,
  /^poly[_-]?signature$/i,
  /^cookie$/i,
  /^authorization$/i,
  /^poly[_-]?private[_-]?key$/i,
  /^polymarket[_-]?private[_-]?key$/i,
  /^polymarket[_-]?api[_-]?secret$/i,
  /^polymarket[_-]?api[_-]?passphrase$/i,
];

function shouldRedactKey(key) {
  return REDACT_KEY_PATTERNS.some((re) => re.test(key));
}

function redactRecord(record) {
  if (Array.isArray(record)) return record.map(redactRecord);
  if (record && typeof record === "object") {
    const out = {};
    for (const [k, v] of Object.entries(record)) {
      if (shouldRedactKey(k)) {
        out[k] = "<redacted>";
      } else if (typeof v === "string") {
        out[k] = redactSecretsInString(v);
      } else {
        out[k] = redactRecord(v);
      }
    }
    return out;
  }
  if (typeof record === "string") return redactSecretsInString(record);
  return record;
}

function httpsRequest(target, method, headers, bodyString) {
  return new Promise((resolve, reject) => {
    const url = new URL(target);
    const req = https.request(
      {
        protocol: url.protocol,
        hostname: url.hostname,
        port: url.port || undefined,
        path: `${url.pathname}${url.search}`,
        method,
        headers: {
          ...headers,
          "User-Agent": USER_AGENT,
          Accept: "*/*",
          "Content-Type": "application/json",
          ...(bodyString != null ? { "Content-Length": Buffer.byteLength(bodyString).toString() } : {}),
        },
        agent: new https.Agent({ keepAlive: false }),
      },
      (res) => {
        let body = "";
        res.setEncoding("utf8");
        res.on("data", (chunk) => {
          body += chunk;
        });
        res.on("end", () => {
          resolve({ status: res.statusCode ?? 0, headers: res.headers, body });
        });
      },
    );
    req.on("error", reject);
    if (bodyString != null) req.write(bodyString);
    req.end();
  });
}

async function buildL1Headers(wallet, chainId, nonce, ts) {
  const nonceBn = BigInt(nonce);
  const value = {
    address: wallet.address,
    timestamp: ts.toString(),
    nonce: nonceBn,
    message: MSG_TO_SIGN,
  };
  const domain = { name: "ClobAuthDomain", version: "1", chainId };
  const types = {
    ClobAuth: [
      { name: "address", type: "address" },
      { name: "timestamp", type: "string" },
      { name: "nonce", type: "uint256" },
      { name: "message", type: "string" },
    ],
  };
  const sig = await wallet._signTypedData(domain, types, value);
  return {
    POLY_ADDRESS: wallet.address,
    POLY_SIGNATURE: sig,
    POLY_TIMESTAMP: ts.toString(),
    POLY_NONCE: nonce.toString(),
  };
}

function buildL2Headers(wallet, creds, method, requestPath, body, ts) {
  const message = `${ts}${method}${requestPath}${body ?? ""}`;
  const secretBytes = Buffer.from(creds.secret, "base64");
  const sig = crypto.createHmac("sha256", secretBytes).update(message).digest("base64");
  const sigUrlSafe = sig.split("+").join("-").split("/").join("_");
  return {
    POLY_ADDRESS: wallet.address,
    POLY_SIGNATURE: sigUrlSafe,
    POLY_TIMESTAMP: ts.toString(),
    POLY_API_KEY: creds.key,
    POLY_PASSPHRASE: creds.passphrase,
  };
}

async function getServerTime(host) {
  const t0 = nowMs();
  const res = await httpsRequest(`${host}/time`, "GET", {});
  return {
    name: "server_time",
    http_status: res.status,
    ms_elapsed: nowMs() - t0,
    server_time: res.body.trim(),
    ok: res.status === 200,
    error: res.status !== 200 ? `status=${res.status} body=${compactBody(res.body)}` : null,
  };
}

async function tryDerive(host, wallet, chainId, nonce, ts) {
  const t0 = nowMs();
  const headers = await buildL1Headers(wallet, chainId, nonce, ts);
  const res = await httpsRequest(`${host}/auth/derive-api-key`, "GET", headers);
  let creds = null;
  let parsed_error = null;
  if (res.status === 200) {
    try {
      const body = JSON.parse(res.body);
      if (body && body.apiKey && body.secret && body.passphrase) {
        creds = { key: body.apiKey, secret: body.secret, passphrase: body.passphrase };
      }
    } catch (err) {
      parsed_error = `parse: ${err.message}`;
    }
  }
  return {
    name: `derive_nonce_${nonce}`,
    http_status: res.status,
    ms_elapsed: nowMs() - t0,
    ok: creds != null,
    creds_present: creds != null,
    creds_key_prefix: creds ? creds.key.slice(0, 4) + "..." : null,
    error:
      creds == null
        ? `status=${res.status} body=${compactBody(redactSecretsInString(res.body))}${parsed_error ? ` ${parsed_error}` : ""}`
        : null,
    creds,
  };
}

async function tryCreate(host, wallet, chainId, nonce, ts) {
  const t0 = nowMs();
  const headers = await buildL1Headers(wallet, chainId, nonce, ts);
  const res = await httpsRequest(`${host}/auth/api-key`, "POST", headers);
  let creds = null;
  let parsed_error = null;
  if (res.status === 200) {
    try {
      const body = JSON.parse(res.body);
      if (body && body.apiKey && body.secret && body.passphrase) {
        creds = { key: body.apiKey, secret: body.secret, passphrase: body.passphrase };
      } else if (body && body.key && body.secret && body.passphrase) {
        creds = body;
      }
    } catch (err) {
      parsed_error = `parse: ${err.message}`;
    }
  }
  return {
    name: `create_nonce_${nonce}`,
    http_status: res.status,
    ms_elapsed: nowMs() - t0,
    ok: creds != null,
    creds_present: creds != null,
    creds_key_prefix: creds ? creds.key.slice(0, 4) + "..." : null,
    error:
      creds == null
        ? `status=${res.status} body=${compactBody(redactSecretsInString(res.body))}${parsed_error ? ` ${parsed_error}` : ""}`
        : null,
    creds,
  };
}

async function l2GetBalanceAllowance(host, wallet, creds, signatureType) {
  const ts = Math.floor(Date.now() / 1000);
  const requestPath = "/balance-allowance";
  const headers = buildL2Headers(wallet, creds, "GET", requestPath, "", ts);
  const url = new URL(`${host}${requestPath}`);
  url.searchParams.set("asset_type", "COLLATERAL");
  url.searchParams.set("signature_type", String(signatureType));
  const t0 = nowMs();
  const res = await httpsRequest(url.toString(), "GET", headers);
  let balance = null;
  if (res.status === 200) {
    try {
      const body = JSON.parse(res.body);
      if (typeof body.balance === "string") balance = body.balance;
    } catch {}
  }
  return {
    http_status: res.status,
    ms_elapsed: nowMs() - t0,
    ok: res.status === 200 && balance != null,
    balance_present: balance != null,
    error:
      res.status !== 200 || balance == null
        ? `status=${res.status} body=${compactBody(redactSecretsInString(res.body))}`
        : null,
  };
}

async function l2GetOpenOrders(host, wallet, creds) {
  const ts = Math.floor(Date.now() / 1000);
  const requestPath = "/data/orders";
  const headers = buildL2Headers(wallet, creds, "GET", requestPath, "", ts);
  const url = new URL(`${host}${requestPath}`);
  url.searchParams.set("next_cursor", "MA==");
  const t0 = nowMs();
  const res = await httpsRequest(url.toString(), "GET", headers);
  let dataLen = null;
  if (res.status === 200) {
    try {
      const body = JSON.parse(res.body);
      if (Array.isArray(body.data)) dataLen = body.data.length;
    } catch {}
  }
  return {
    http_status: res.status,
    ms_elapsed: nowMs() - t0,
    ok: res.status === 200 && dataLen != null,
    data_length: dataLen,
    error:
      res.status !== 200 || dataLen == null
        ? `status=${res.status} body=${compactBody(redactSecretsInString(res.body))}`
        : null,
  };
}

async function main() {
  const args = parseArgs(process.argv);
  const env = process.env;
  const result = {
    kind: "buba_clob_auth_probe",
    started_at_utc: utcNow(),
    finished_at_utc: null,
    chain_id: POLYGON_CHAIN_ID,
    host: env.POLYMARKET_CLOB_HOST || DEFAULT_HOST,
    iterations: args.iterations,
    nonce_scan: args.nonceScan,
    try_create: args.tryCreate,
    signature_types_tested: null,
    signer_address: null,
    proxy_wallet: env.POLYMARKET_PROXY_WALLET || null,
    funder: env.POLYMARKET_FUNDER || env.POLYMARKET_PROXY_WALLET || null,
    configured_signature_type: env.POLYMARKET_SIGNATURE_TYPE
      ? Number.parseInt(env.POLYMARKET_SIGNATURE_TYPE, 10)
      : 1,
    preconfigured_creds_present: Boolean(
      env.POLYMARKET_API_KEY && env.POLYMARKET_API_SECRET && env.POLYMARKET_API_PASSPHRASE,
    ),
    configured_nonce_hint: env.POLYMARKET_API_KEY_NONCE
      ? Number.parseInt(env.POLYMARKET_API_KEY_NONCE, 10)
      : null,
    server_time: null,
    derive_attempts: [],
    create_attempts: [],
    chosen_creds_source: null,
    chosen_creds_key_prefix: null,
    stability: {
      total: 0,
      passed_balance: 0,
      passed_orders: 0,
      failed_balance: 0,
      failed_orders: 0,
      avg_ms_balance: null,
      avg_ms_orders: null,
      first_balance_failure: null,
      first_orders_failure: null,
      iterations: [],
    },
    signature_type_probe: null,
    summary: {
      passed: false,
      blockers: [],
    },
  };

  if (!env.POLYMARKET_PRIVATE_KEY) {
    result.summary.blockers.push("missing POLYMARKET_PRIVATE_KEY");
    await writeOutput(args.out, result);
    process.exit(2);
  }

  const wallet = new Wallet(env.POLYMARKET_PRIVATE_KEY);
  result.signer_address = wallet.address;

  const host = result.host.replace(/\/$/, "");

  // Test 1: server time
  result.server_time = await getServerTime(host);
  if (!result.server_time.ok) {
    result.summary.blockers.push("server_time_failed");
  }

  // Test 2: choose creds
  let creds = null;
  let credsSource = null;
  let chosenNonce = null;
  if (result.preconfigured_creds_present) {
    creds = {
      key: env.POLYMARKET_API_KEY,
      secret: env.POLYMARKET_API_SECRET,
      passphrase: env.POLYMARKET_API_PASSPHRASE,
    };
    credsSource = "preconfigured";
  }

  // Test 3: derive scan (always run, even when preconfigured, for diagnostic comparison)
  for (const nonce of args.nonceScan) {
    const ts = Math.floor(Date.now() / 1000);
    try {
      const attempt = await tryDerive(host, wallet, POLYGON_CHAIN_ID, nonce, ts);
      const { creds: c, ...redacted } = attempt;
      result.derive_attempts.push(redacted);
      if (!creds && c) {
        creds = c;
        credsSource = `derived_nonce_${nonce}`;
        chosenNonce = nonce;
      }
    } catch (err) {
      result.derive_attempts.push({
        name: `derive_nonce_${nonce}`,
        ok: false,
        error: redactSecretsInString(err.message),
      });
    }
  }

  // Test 4: create scan when no creds yet, OR force-create requested
  if ((args.forceCreate || !creds) && args.tryCreate) {
    for (const nonce of args.nonceScan) {
      const ts = Math.floor(Date.now() / 1000);
      try {
        const attempt = await tryCreate(host, wallet, POLYGON_CHAIN_ID, nonce, ts);
        const { creds: c, ...redacted } = attempt;
        result.create_attempts.push(redacted);
        if (!creds && c) {
          creds = c;
          credsSource = `created_nonce_${nonce}`;
          chosenNonce = nonce;
        }
      } catch (err) {
        result.create_attempts.push({
          name: `create_nonce_${nonce}`,
          ok: false,
          error: redactSecretsInString(err.message),
        });
      }
    }
  }

  result.chosen_creds_source = credsSource;
  result.chosen_creds_key_prefix = creds ? creds.key.slice(0, 4) + "..." : null;

  if (!creds) {
    result.summary.blockers.push("no_l2_credentials_available");
    await writeOutput(args.out, result);
    summarize(result);
    process.exit(3);
  }

  // Test 5: probe signature_type both 1 and 2 against /balance-allowance once
  const signatureTypeCandidates = args.signatureTypes ?? [result.configured_signature_type];
  if (!args.signatureTypes && result.configured_signature_type !== 2) {
    signatureTypeCandidates.push(2);
  }
  result.signature_types_tested = signatureTypeCandidates;
  const sigTypeResults = {};
  for (const st of signatureTypeCandidates) {
    sigTypeResults[`type_${st}`] = await l2GetBalanceAllowance(host, wallet, creds, st);
  }
  result.signature_type_probe = sigTypeResults;
  const workingSignatureTypes = Object.entries(sigTypeResults)
    .filter(([, v]) => v.ok)
    .map(([k]) => Number.parseInt(k.split("_")[1], 10));
  result.working_signature_types = workingSignatureTypes;
  if (workingSignatureTypes.length === 0) {
    result.summary.blockers.push("no_signature_type_works_for_balance_allowance");
  }

  // Test 6: stability loop using the configured signature type (or first working)
  const stabilitySigType =
    workingSignatureTypes.includes(result.configured_signature_type)
      ? result.configured_signature_type
      : workingSignatureTypes[0] ?? result.configured_signature_type;
  result.stability_signature_type = stabilitySigType;
  let totalBalanceMs = 0;
  let totalOrdersMs = 0;
  for (let i = 1; i <= args.iterations; i += 1) {
    const balance = await l2GetBalanceAllowance(host, wallet, creds, stabilitySigType);
    const orders = await l2GetOpenOrders(host, wallet, creds);
    const iter = {
      i,
      balance: { http_status: balance.http_status, ms_elapsed: balance.ms_elapsed, ok: balance.ok, error: balance.error },
      orders: { http_status: orders.http_status, ms_elapsed: orders.ms_elapsed, ok: orders.ok, error: orders.error },
    };
    result.stability.iterations.push(iter);
    result.stability.total += 1;
    if (balance.ok) {
      result.stability.passed_balance += 1;
      totalBalanceMs += balance.ms_elapsed;
    } else {
      result.stability.failed_balance += 1;
      if (!result.stability.first_balance_failure) result.stability.first_balance_failure = iter.balance;
    }
    if (orders.ok) {
      result.stability.passed_orders += 1;
      totalOrdersMs += orders.ms_elapsed;
    } else {
      result.stability.failed_orders += 1;
      if (!result.stability.first_orders_failure) result.stability.first_orders_failure = iter.orders;
    }
  }
  if (result.stability.passed_balance > 0) {
    result.stability.avg_ms_balance = Math.round(totalBalanceMs / result.stability.passed_balance);
  }
  if (result.stability.passed_orders > 0) {
    result.stability.avg_ms_orders = Math.round(totalOrdersMs / result.stability.passed_orders);
  }

  if (result.stability.failed_balance > 0) {
    result.summary.blockers.push(
      `balance_allowance_unstable failed=${result.stability.failed_balance}/${result.stability.total}`,
    );
  }
  if (result.stability.failed_orders > 0) {
    result.summary.blockers.push(
      `data_orders_unstable failed=${result.stability.failed_orders}/${result.stability.total}`,
    );
  }

  result.summary.passed = result.summary.blockers.length === 0;
  result.finished_at_utc = utcNow();

  await writeOutput(args.out, result);
  summarize(result);
  process.exit(result.summary.passed ? 0 : 1);
}

async function writeOutput(outPath, result) {
  const dir = path.dirname(outPath);
  await fs.mkdir(dir, { recursive: true });
  const safe = redactRecord(result);
  await fs.writeFile(outPath, JSON.stringify(safe, null, 2) + "\n", "utf8");
}

function summarize(result) {
  const lines = [
    `host=${result.host}`,
    `signer=${result.signer_address}`,
    `proxy=${result.proxy_wallet}`,
    `funder=${result.funder}`,
    `signature_type_configured=${result.configured_signature_type}`,
    `server_time_ok=${result.server_time?.ok ?? false}`,
    `creds_source=${result.chosen_creds_source}`,
    `working_signature_types=${(result.working_signature_types || []).join(",") || "none"}`,
    `stability total=${result.stability.total} balance_pass=${result.stability.passed_balance} orders_pass=${result.stability.passed_orders} avg_balance_ms=${result.stability.avg_ms_balance ?? "n/a"} avg_orders_ms=${result.stability.avg_ms_orders ?? "n/a"}`,
    `passed=${result.summary.passed} blockers=${result.summary.blockers.join("|") || "none"}`,
  ];
  for (const line of lines) {
    process.stderr.write(`probe ${line}\n`);
  }
}

main().catch((err) => {
  process.stderr.write(`probe_error ${redactSecretsInString(err.stack || err.message)}\n`);
  process.exit(99);
});
