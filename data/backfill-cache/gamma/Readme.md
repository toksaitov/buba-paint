# Gamma Backfill Cache

This directory contains cached Gamma API responses for BTC up/down 5 minute market slugs.

Files are named by market slug, for example `btc-updown-5m-1774774200.json`. They are used by historical run upgrade and settlement verification workflows to avoid repeatedly fetching the same metadata from Gamma.

This is derived cache data. It is safe to regenerate, but should not be casually deleted while working on historical upgrades because it makes those workflows faster and more reproducible.
