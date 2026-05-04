CLIENT_DIR := dashboard/client
SIDECAR_DIR := polymarket-sidecar
COMMENT_POLICY := cargo run --quiet --manifest-path tools/rust-comment-policy/Cargo.toml --
TS_COMMENT_AUDIT := node scripts/ts_comment_audit.mjs

.PHONY: lint comment-audit docs-audit live-readiness-local live-readiness-host-soak docker-deploy docker-deploy-dry-run sidecar-lint sidecar-test sidecar-build sidecar-audit test-fast test-integration test-slow test-e2e test-all coverage coverage-gate

lint:
	cargo fmt --all --check
	cargo clippy --workspace -- -D warnings
	$(COMMENT_POLICY) check
	$(TS_COMMENT_AUDIT) check
	cd $(SIDECAR_DIR) && npm run lint

sidecar-lint:
	cd $(SIDECAR_DIR) && npm run lint

sidecar-test:
	cd $(SIDECAR_DIR) && npm test

sidecar-build:
	cd $(SIDECAR_DIR) && npm run build

sidecar-audit:
	cd $(SIDECAR_DIR) && npm run audit:security

comment-audit:
	$(COMMENT_POLICY) report
	$(TS_COMMENT_AUDIT) report

docs-audit:
	python3 scripts/audit-docs.py

LIVE_READINESS_ARGS ?=
live-readiness-local:
	python3 scripts/live-readiness-local.py $(LIVE_READINESS_ARGS)

LIVE_HOST_SOAK_ARGS ?=
live-readiness-host-soak:
	python3 scripts/live-readiness-host-soak.py $(LIVE_HOST_SOAK_ARGS)

DOCKER_DEPLOY_ARGS ?= --host buba-paint --domain buba.toksaitov.com --mode live-readonly --install-docker
docker-deploy:
	python3 scripts/deploy-docker.py $(DOCKER_DEPLOY_ARGS)

docker-deploy-dry-run:
	python3 scripts/deploy-docker.py $(DOCKER_DEPLOY_ARGS) --dry-run

test-fast:
	cargo test --workspace --lib
	cd $(CLIENT_DIR) && npm test
	cd $(SIDECAR_DIR) && npm test

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
