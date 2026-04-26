# Run 012 Sweep Blocked

The full run 012 sweep was not started.

Reason: the exact-params baseline replay failed the parity gate even after raw-event replay and native-window-open fixes.

Archived run 012 produced `$701.50` final balance, `180` trades, and `50.04%` max drawdown. The latest fixed replay on the derived run 012 DB produced `$156.12` final balance, `142` trades, and `42.8%` max drawdown.

This is too far from the archived run to trust a parameter sweep. Optimizing on this replay would optimize a different event stream than the one that generated run 012.

The remaining root cause is missing Binance `bookTicker` persistence in the compact run archive. The live runtime used book-ticker state in memory, but run 012 persisted zero Binance `bookTicker` feed-event rows.

The new pre-sweep data-quality gate should now block this kind of input automatically.

See `data/experiments/run-012-forensics-001/baseline_parity.md`, `data/experiments/run-012-forensics-001/parity_diagnostics.md`, and `data/experiments/run-012-forensics-001/postmortem.md`.
