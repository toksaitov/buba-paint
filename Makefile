CLIENT_DIR := dashboard/client
COMMENT_POLICY := cargo run --quiet --manifest-path tools/rust-comment-policy/Cargo.toml --
TS_COMMENT_AUDIT := node scripts/ts_comment_audit.mjs

.PHONY: lint comment-audit test-fast test-integration test-slow test-e2e test-all coverage coverage-gate

lint:
	cargo fmt --all --check
	cargo clippy --workspace -- -D warnings
	$(COMMENT_POLICY) check
	$(TS_COMMENT_AUDIT) check

comment-audit:
	$(COMMENT_POLICY) report
	$(TS_COMMENT_AUDIT) report

test-fast:
	cargo test --workspace --lib
	cd $(CLIENT_DIR) && npm test

test-integration:
	cargo test -p buba-agent --test integration_test
	cargo test -p buba-dashboard --test integration_test
	cargo test -p buba-paint --test backtest_test
	cargo test -p buba-paint --test build_data_test
	cargo test -p buba-paint --test cli_test
	cargo test -p buba-paint --test discovery_test
	cargo test -p buba-paint --test feeds_test

test-slow:
	cargo test -p buba-paint --test live_system_test

test-e2e:
	cd $(CLIENT_DIR) && npm run test:e2e

test-all: test-fast test-integration test-slow test-e2e

coverage:
	cargo llvm-cov --summary-only -p buba-paint --lib --tests --ignore-filename-regex 'main\.rs$$'
	cargo llvm-cov --summary-only -p buba-agent --lib --tests --ignore-filename-regex 'main\.rs$$'
	cargo llvm-cov --summary-only -p buba-dashboard --lib --tests --ignore-filename-regex 'main\.rs$$'
	cd $(CLIENT_DIR) && npm run test:coverage

coverage-gate:
	python3 scripts/check_coverage.py
