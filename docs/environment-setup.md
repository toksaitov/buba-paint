# Environment Setup

A from-scratch setup checklist for a fresh macOS machine on Apple Silicon (arm64)
so you can build, test, publish images, and deploy this project. Follow it top to
bottom. Command details and environment knobs live in
[docs/commands-and-config.md](./commands-and-config.md); deployment specifics live
in [docs/deployment-and-ops.md](./deployment-and-ops.md) and
[ops/docker/Readme.md](../ops/docker/Readme.md).

## Prerequisites

Install these first. Homebrew is the simplest source for most of them.

* Rust via rustup. The workspace uses edition 2024 and sets
  `rust-version = "1.94"` as the minimum supported version (see
  `bots/paint/Cargo.toml`). There is no `rust-toolchain.toml`, so install the
  current stable toolchain and keep it at 1.94 or newer.
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  rustup default stable
  rustc --version   # expect 1.94.0 or newer
  ```
* Node.js and npm. The repo pins no Node version (no `engines` field and no
  `.nvmrc`), but the frontend toolchain needs a modern LTS: `dashboard/client`
  depends on Vite 8 (needs Node 20.19+ or 22.12+) and both Node packages depend
  on `@types/node` 24. Use Node 22 LTS or newer.
  ```bash
  brew install node@22   # or use nvm/fnm to install 22 LTS
  node --version
  npm --version
  ```
* Docker Desktop for Mac (Apple Silicon). Needed for image builds, `docker
  buildx`, and Compose. Launch it once so the daemon and the `desktop-linux`
  context are created.
* GitHub CLI (`gh`). Used by the image publish and deploy scripts for GHCR auth
  and token scopes.
  ```bash
  brew install gh
  gh auth login
  ```
* git and Git LFS. The repo stores run evidence through LFS (see
  `.gitattributes`: `runs/**`, `data/*.db`, and related paths use
  `filter=lfs`), so LFS must be installed before cloning or those files arrive as
  pointer stubs.
  ```bash
  brew install git git-lfs
  git lfs install
  ```

## Moving An Existing Checkout (do this, not a fresh clone)

If you are moving to a new machine to continue this work, copy the whole working
directory. Do not re-clone. The active work is on the `live-readiness` branch,
which is not pushed to any remote (it is about 25 commits ahead of
`origin/master`). A plain `git clone` gives you `master`, which does not contain
the canary, the `live_trading` compose, the hardened live bootstrap, or these
handoff docs. A clone also drops the gitignored `.secrets/` directory and any
local run evidence.

Copy the entire directory including `.git` and `.secrets`, skipping only the
large regenerable build outputs:

```bash
rsync -aH --info=progress2 \
  --exclude 'target/' \
  --exclude 'node_modules/' \
  --exclude 'dist/' \
  --exclude '.docker/' \
  /path/to/buba-paint/ new-machine:/Users/<you>/Desktop/buba-paint/
```

`.git` carries the unpushed `live-readiness` branch and full history; `.secrets/`
carries the live sidecar env, so keep the transfer secure. On the new machine,
confirm the state before doing anything else:

```bash
cd buba-paint
git status                # expect: On branch live-readiness, working tree clean
git log --oneline -3      # expect the latest live-readiness commits
chmod 600 .secrets/buba-paint-live-sidecar.env
```

Then run the per-package install below to rebuild `target/` and `node_modules/`
(intentionally not copied). Start context reconstruction from
[LIVE_READINESS_PLAN.md](../LIVE_READINESS_PLAN.md) (the handoff block at the top)
and the read order in [CLAUDE.md](../CLAUDE.md).

If you prefer git over a whole-directory copy, push the branch from the old
machine first (`git push -u origin live-readiness`) and clone it explicitly
(`git clone -b live-readiness ...`); you still must copy `.secrets/`
out-of-band because it is gitignored. Caution: a push plus clone carries only
committed history. Run `git status` on the old machine first and commit or stash
any modified or untracked files (and copy any local run evidence separately), or
the clone silently loses them. The whole-directory rsync above avoids this
because it copies the working tree as-is.

## Clone And Per-Package Install

If you moved an existing checkout with the rsync above, skip the `git clone` and
`git lfs pull` here and run only the build and install commands. For a genuinely
fresh setup, clone with LFS active, then install each crate and package.

```bash
git clone -b live-readiness https://github.com/toksaitov/buba-paint.git
cd buba-paint
git lfs pull

# Rust workspace (bot, agent, dashboard server, telemetry crate)
cargo build

# Polymarket sidecar (TypeScript). Lockfile is committed, so use npm ci.
cd polymarket-sidecar && npm ci && cd ..

# Dashboard client (React). Lockfile is committed, so use npm ci.
cd dashboard/client && npm ci && cd ../..
```

## Gotcha: rustup Proxy Breaks cargo Subcommands And make

On this setup the rustup proxies under `~/.cargo/bin` can fail to dispatch cargo
subcommands, so `cargo fmt`, `cargo clippy`, and `cargo test` (and any `make`
target that shells out to them) error out. The fix is to put the real stable
toolchain bin directory ahead of `~/.cargo/bin` on `PATH`. This also fixes
`make`.

```bash
export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"
```

Add that line to your shell profile (`~/.zshrc`) so it persists. After it,
`cargo fmt --all --check`, `cargo clippy --workspace -- -D warnings`,
`cargo test`, and `make lint` / `make test-fast` all resolve correctly.

## Gotcha: Image Publish Hangs At "docker login ghcr.io"

`scripts/publish-live-images.py` runs `docker login ghcr.io ...` and then
`docker buildx build --push` for each image. With Docker Desktop's default
config, `credsStore` is set to `"desktop"`, and the `docker-credential-desktop`
helper blocks when invoked from a non-interactive shell, so the publish hangs at
the login step.

Work around it by building a throwaway `DOCKER_CONFIG` directory that has the
credential helpers stripped and an inline GHCR auth entry, then symlink in the
plugin and context directories Docker still needs.

```bash
# 1. GHCR identity from gh
GH_USER="$(gh api user -q .login)"
GH_TOKEN="$(gh auth token)"

# 2. Throwaway config dir
export DOCKER_CONFIG="$(mktemp -d)"

# 3. Base64 of "<gh-username>:<gh-token>" for the inline auth entry
AUTH="$(printf '%s:%s' "$GH_USER" "$GH_TOKEN" | base64)"

# 4. Copy the real config, drop credsStore/credHelpers, seed the ghcr.io auth.
#    Requires jq (brew install jq).
jq --arg auth "$AUTH" \
  'del(.credsStore, .credHelpers) | .auths = ((.auths // {}) + {"ghcr.io": {"auth": $auth}})' \
  ~/.docker/config.json > "$DOCKER_CONFIG/config.json"

# 5. Symlink cli-plugins so `docker buildx` is available, and contexts so the
#    desktop-linux context still resolves.
ln -s ~/.docker/cli-plugins "$DOCKER_CONFIG/cli-plugins"
ln -s ~/.docker/contexts    "$DOCKER_CONFIG/contexts"

# 6. Publish with the throwaway config in scope
DOCKER_CONFIG="$DOCKER_CONFIG" python3 scripts/publish-live-images.py
```

Without the `cli-plugins` symlink, `docker buildx` is not found; without the
`contexts` symlink, the `desktop-linux` context is missing. With `credsStore`
and `credHelpers` removed and the inline `ghcr.io` auth present, `docker login`
writes to the config file instead of calling the blocking helper, so the publish
proceeds.

## gh Auth Scopes For GHCR

GHCR access is gated on token scopes, and the scripts check them.

* Pulling digest-locked images during a deploy (`deploy-docker.py
  --use-locked-images`, and the stopped-live and machine deploys) needs
  `read:packages`. Without it the script fails with
  `gh token lacks read:packages`.
* Publishing images (`scripts/publish-live-images.py`) needs `write:packages`.
  Without it the script fails with `gh token lacks write:packages`.

Grant both at once, then verify:

```bash
gh auth refresh -s read:packages -s write:packages
gh auth status
```

`gh auth status` prints the token scopes; confirm both `read:packages` and
`write:packages` appear.

## SSH Host Aliases

The ops and deploy scripts address hosts by SSH alias, not by IP. Two aliases
must exist in `~/.ssh/config` on the new machine:

* `buba-paint`: the Ireland canary and production host. It is the default
  `--host` for `scripts/deploy-docker.py` and the target for the live and
  stopped-live deploys.
* `testing`: the research host (Ubuntu on WSL). It is the target for the
  research machine deploy and the research maintenance commands.

Example `~/.ssh/config` entries (fill in your real hostnames, users, and keys):

```
Host buba-paint
    HostName <ireland-host>
    User <user>
    IdentityFile ~/.ssh/<key>

Host testing
    HostName <research-host>
    User <user>
    IdentityFile ~/.ssh/<key>
```

Verify connectivity before any deploy work:

```bash
ssh buba-paint uptime
ssh testing uptime
```

## Secrets

Live and sidecar deploys read `.secrets/buba-paint-live-sidecar.env`. It is the
default `--sidecar-env` for `scripts/deploy-docker.py` and is required whenever
the deploy mode is `live-readonly` or `live-trading` (the script aborts with
`missing sidecar env` if it is absent).

Important facts about this file:

* The whole `.secrets/` directory is gitignored (`/.secrets/` in
  `.gitignore`), so it does not arrive with a `git clone` and no template ships
  inside it. There is no committed `.secrets/buba-paint-live-sidecar.env.example`.
* You must copy the real file onto the new machine out-of-band (from a secure
  transfer or your password manager), place it at
  `.secrets/buba-paint-live-sidecar.env`, and lock its permissions:
  ```bash
  mkdir -p .secrets
  # copy the file into place out-of-band, then:
  chmod 600 .secrets/buba-paint-live-sidecar.env
  ```
* For the variable reference, use the sidecar and Polymarket sections of the
  repo-root [.env.example](../.env.example) (the `SIDECAR_*` and `POLYMARKET_*`
  keys). The live secret file must define at least: `SIDECAR_PORT`,
  `SIDECAR_HOST`, `POLYMARKET_GEOBLOCK_URL`, `POLYMARKET_CLOB_HOST`,
  `POLYMARKET_RELAYER_HOST`, `POLYMARKET_CLOCK_DRIFT_MAX_MS`,
  `POLYMARKET_SIGNATURE_TYPE`, `POLYMARKET_PRIVATE_KEY`,
  `POLYMARKET_PROXY_WALLET`, `POLYMARKET_FUNDER`, `BUBA_SIDECAR_LOG_PATH`,
  `POLYMARKET_RELAYER_API_KEY`, `POLYMARKET_RELAYER_API_KEY_ADDRESS`,
  `POLYMARKET_API_KEY`, `POLYMARKET_API_SECRET`, `POLYMARKET_API_PASSPHRASE`,
  and `POLYMARKET_API_KEY_NONCE`.

Paper mode does not need this file; only the sidecar-backed modes do.

## Quick Verification

Run these to confirm the machine is ready. The `make` targets assume the rustup
PATH fix above is in effect.

```bash
cargo build
make lint
make test-fast
cd polymarket-sidecar && npm test && cd ..
cd dashboard/client && npm run build && cd ../..
ssh buba-paint uptime
```

If all of these succeed, the environment can build the workspace, run the fast
test gate, exercise the sidecar tests, produce a dashboard build, and reach the
deploy host. For the full local gate set (lint, test-all, release build, and
dashboard build) before server work, see the Build And Test section of
[CLAUDE.md](../CLAUDE.md).
