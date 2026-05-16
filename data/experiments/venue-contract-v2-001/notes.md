# Venue Contract V2 Host Check

Checked from host `buba-paint` on `2026-05-01T09:43:21Z`.

This was a no-order, non-deploying public endpoint check. It did not use account secrets and did not submit orders, cancellations, or redemptions.

## Result

* Polymarket geoblock endpoint returned `200` with `blocked=false` and country `IE`.
* Gamma BTC 5-minute event discovery returned `403` with body `error code: 1010` for the current and next slot.
* CLOB V2 `/clob-markets/{conditionId}` metadata could not be checked because Gamma did not return condition IDs.
* Authenticated CLOB V2 account, preflight, and user-stream checks were not run because the updated V2 sidecar was not deployed to the host in this phase.

## Interpretation

The host is not geoblocked by the Polymarket geoblock endpoint, but Gamma discovery is currently blocked by a separate `403` response from the deployment host. This is a live-money blocker until resolved or until market discovery has a verified host-safe fallback.

Do not arm real money from this host until a no-order host check confirms BTC 5-minute market discovery, CLOB V2 market metadata, account state, and user-stream readiness with the updated sidecar.
