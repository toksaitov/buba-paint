#!/usr/bin/env python3
"""Probe live Polymarket venue assumptions through the running sidecar.

Read-only. Calls the sidecar ``/preflight`` endpoint, extracts the venue
assumptions the live path depends on (fee rate and exponent, min order size,
tick size, collateral, signature type, hosts, exchange domain version), and
writes a no-secret JSON artifact to an ignored path for the live-readiness
checklist. Run this at the Phase 0 operator gate against a sidecar configured
with real read-only credentials. It never sends an order and never prints
secrets.
"""
import argparse
import json
import os
import time
import urllib.request

DEFAULT_SIDECAR = "http://127.0.0.1:3210"
DEFAULT_CLOB = "https://clob.polymarket.com"
DEFAULT_GAMMA = "https://gamma-api.polymarket.com"
DEFAULT_OUT = "/tmp/buba-live-readiness/venue-assumptions.json"
EXPECTED_EXCHANGE_DOMAIN_VERSION = "2"


def fetch_preflight(sidecar_url, clob_api_url, gamma_api_url):
    """POST a minimal read-only preflight request and return the parsed response."""
    body = json.dumps(
        {
            "execution_mode": "live_readonly",
            "clob_api_url": clob_api_url,
            "gamma_api_url": gamma_api_url,
            "strategy_readiness": [],
            "budget_limits": {
                "cash_cap_usd": 1.0,
                "max_single_order_usd": 1.0,
                "max_open_notional_usd": 1.0,
                "max_daily_loss_usd": 1.0,
                "max_session_drawdown_usd": 1.0,
                "min_required_cash_usd": 0.0,
            },
        }
    ).encode("utf-8")
    request = urllib.request.Request(
        f"{sidecar_url.rstrip('/')}/preflight",
        data=body,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.loads(response.read().decode("utf-8"))


def extract_assumptions(preflight, clob_api_url, gamma_api_url):
    """Reduce a preflight response to the no-secret venue-assumptions record."""
    details = {}
    raw = preflight.get("details_json")
    if isinstance(raw, str):
        try:
            details = json.loads(raw)
        except json.JSONDecodeError:
            details = {}
    markets = []
    for market in details.get("active_markets", []) or []:
        fee = market.get("feeDetails") or {}
        markets.append(
            {
                "slug": market.get("slug"),
                "condition_id": market.get("conditionId"),
                "fee_rate": fee.get("rate"),
                "fee_exponent": fee.get("exponent"),
                "fee_taker_only": fee.get("takerOnly"),
                "min_order_size": market.get("minOrderSize"),
                "tick_size": market.get("tickSize"),
                "neg_risk": market.get("negRisk"),
                "metadata_source": market.get("metadataSource"),
            }
        )
    return {
        "captured_at_ms": int(time.time() * 1000),
        "preflight_ok": preflight.get("ok"),
        "signature_type": preflight.get("signature_type"),
        "geoblock_status": preflight.get("geoblock_status"),
        "geoblock_country_code": preflight.get("geoblock_country_code"),
        "geoblock_ip": preflight.get("geoblock_ip"),
        "clob_contract_version": details.get("clob_contract_version"),
        "collateral_token": details.get("collateral_token"),
        "collateral_decimals": details.get("collateral_decimals"),
        "expected_exchange_domain_version": EXPECTED_EXCHANGE_DOMAIN_VERSION,
        "clob_api_url": clob_api_url,
        "gamma_api_url": gamma_api_url,
        "markets": markets,
    }


def main():
    """Parse arguments, probe the sidecar, and write the assumptions artifact."""
    parser = argparse.ArgumentParser(
        description="Probe live Polymarket venue assumptions (read-only)."
    )
    parser.add_argument(
        "--sidecar-url",
        default=os.environ.get("LIVE_SIDECAR_URL", DEFAULT_SIDECAR),
    )
    parser.add_argument("--clob-api-url", default=DEFAULT_CLOB)
    parser.add_argument("--gamma-api-url", default=DEFAULT_GAMMA)
    parser.add_argument("--output", default=DEFAULT_OUT)
    args = parser.parse_args()

    preflight = fetch_preflight(args.sidecar_url, args.clob_api_url, args.gamma_api_url)
    assumptions = extract_assumptions(preflight, args.clob_api_url, args.gamma_api_url)

    out_dir = os.path.dirname(args.output)
    if out_dir:
        os.makedirs(out_dir, exist_ok=True)
    with open(args.output, "w", encoding="utf-8") as handle:
        json.dump(assumptions, handle, indent=2, sort_keys=True)
        handle.write("\n")
    redacted = dict(assumptions)
    if redacted.get("geoblock_ip") is not None:
        redacted["geoblock_ip"] = "[redacted; see artifact]"
    print(f"wrote venue assumptions to {args.output}")
    print(json.dumps(redacted, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
