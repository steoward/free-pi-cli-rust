# free-pi-cli (Rust)

[![ci](https://github.com/steoward/free-pi-cli-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/steoward/free-pi-cli-rust/actions/workflows/ci.yml)

A Rust rewrite of [free-pi-cli](https://github.com/dennisonbertram/free-pi-cli),
a free, ad-supported distribution of the [pi](https://github.com/earendil-works/pi)
coding agent. This rewrite replaces the TypeScript orchestration layer with a
native Rust CLI while keeping pi's terminal UI as the agent.

```sh
bunx free-pi-cli
```

`bunx` downloads the package, picks the native binary for your OS, and starts
free-pi. No `npx`, no Node-only launcher.

First run shows the consent screen (free inference paid for by ads, sessions
may be used to train models), then signs you in with GitHub's device flow.
Later runs reuse the stored token and go straight to the agent.

```sh
bunx free-pi-cli                 # start a new session
bunx free-pi-cli --session <id>  # resume a session in this directory
bunx free-pi-cli logout          # remove the stored token
bunx free-pi-cli mcp <command>   # manage your own MCP servers
bunx free-pi-cli --version
bunx free-pi-cli --help
```

Prefer a shorter command? Install it once:

```sh
bun add -g free-pi-cli
free-pi-cli
```

## How it fits together

| Piece | Role |
|---|---|
| `bin/free-pi-cli.js` | npm/Bun entry shim: finds the platform binary and pi, forwards the terminal and exit code |
| Rust binary (`src/`) | consent, GitHub device-flow login, JWT storage, `/me` token check, `/client-version` gate, config generation, session ids, MCP passthrough |
| `@earendil-works/pi-coding-agent` | the agent and interactive TUI (pinned to `1.5.2`) |
| `assets/free-pi-extension.js` | bundled pi extension: ads, branded header, free-pi tools and commands, Fusion, tool guard |

On every start the Rust CLI:

1. Reads or clears the JWT under `~/.free-pi/agent`.
2. Checks `/me`, re-prompts consent when the server asks for it, and runs the
   GitHub device flow when there is no usable token.
3. Calls `/client-version`, enforces the minimum CLI version, and reads the
   model catalog and Fusion pair.
4. Materializes `models.json`, `settings.json`, `APPEND_SYSTEM.md`, the
   long-jobs skill, and the extension into `~/.free-pi/agent`.
5. Launches pi through Bun with `PI_CODING_AGENT_DIR=~/.free-pi/agent`,
   `FREEPI_JWT`, the session id, and the model catalog in the environment.

The provider is configured as an OpenAI-compatible endpoint at
`https://api.freepi.ai/v1`. `x-session-id` is injected per request by the
extension; `x-client-version` is written into `models.json` and kept in sync
by the same extension.

## Data locations

Everything free-pi owns lives under `~/.free-pi`, separate from `~/.pi`:

| Path | Contents |
|---|---|
| `~/.free-pi/agent/credentials.json` | JWT (mode `0600` where supported) |
| `~/.free-pi/agent/models.json` | free-pi provider catalog |
| `~/.free-pi/agent/settings.json` | defaults, model scope, project-trust prompt |
| `~/.free-pi/agent/extensions/free-pi.js` | bundled extension |
| `~/.free-pi/agent/sessions/--<cwd>--/` | pi session files |
| `~/.free-pi/gravity/` | Gravity ad-measurement state |

## Differences from the TypeScript release

- The orchestration is Rust; pi itself remains the upstream Node/Bun TUI.
- pi is pinned to `1.5.2` instead of the older `1.2.0` used by the TypeScript
  package. The server's current minimum client version is satisfied.
- There is no in-place self-update. `bunx free-pi-cli` already fetches the
  latest version on each launch; `/update` updates a global install with
  `bun add -g free-pi-cli@latest` and otherwise points at `bunx`.
- pi reserves `/bug`, which an extension cannot override, so the feedback
  command is `/bug-report`.
- Project trust uses pi's native prompt (`defaultProjectTrust: "ask"`) instead
  of a separate onboarding dialog.
- The backend's cost and choice A/B experiment arms are not enabled; free-pi
  always runs the default rules and context reserve.
- Linux packages are built against glibc (x64 and ARM64).

## Environment variables

| Variable | Purpose |
|---|---|
| `FREEPI_BASE_URL` | Override the backend base URL (default `https://api.freepi.ai`) |
| `FREEPI_NO_GRAVITY_PIXEL=1` | Do not run Gravity's ad-measurement pixel |
| `FREEPI_KEEP_ENV_PROVIDERS` | Show every provider your environment can authenticate, not just free-pi |
| `FREEPI_CLI_BIN` | Override the native binary the npm shim runs |
| `FREEPI_PI_CLI` | Override the path to pi's `dist/bundle/cli.js` |
| `FREEPI_JS_RUNTIME` | Override the Bun/Node executable used to run pi |

## Development

Requirements: Bun 1.0+, Rust 1.80+, and the platform C toolchain Rust needs.

```sh
bun install --cwd extension
bun run build:extension            # rebuild assets/free-pi-extension.js
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check

cargo build --release
bun bin/free-pi-cli.js --version  # shim finds target/release/free-pi-cli
```

To run the full first-run flow against a local server:

```sh
FREEPI_BASE_URL=http://127.0.0.1:8787 bun bin/free-pi-cli.js
```

On PowerShell:

```powershell
$env:FREEPI_BASE_URL = "http://127.0.0.1:8787"
bun bin/free-pi-cli.js
```

The extension is built from `extension/src` into a single ESM file. Its bare
imports of `@earendil-works/pi-coding-agent`, `@earendil-works/pi-tui`, and
`typebox` stay external because pi's extension loader resolves them to its own
copies.

## Packaging and releases

The npm package is split into a small entry package plus one optional binary
package per platform:

```
free-pi-cli                 bin shim, README, LICENSE
free-pi-cli-win32-x64       native free-pi-cli.exe
free-pi-cli-win32-arm64
free-pi-cli-darwin-x64
free-pi-cli-darwin-arm64
free-pi-cli-linux-x64
free-pi-cli-linux-arm64
```

`scripts/package-npm.mjs` copies a compiled binary into its platform package.
`scripts/verify-release.mjs` checks that `Cargo.toml`, the root package, every
platform package, and the pinned pi version agree.

Every tagged release also attaches those six binaries to the GitHub release as
`free-pi-cli-<platform>` (`free-pi-cli-win32-x64.exe` on Windows), so a binary
can be downloaded directly from
[Releases](https://github.com/steoward/free-pi-cli-rust/releases) without npm.

CI (`.github/workflows/ci.yml`) builds the extension, runs Rust formatting,
clippy, tests, and a release build. Tagging `v<version>` triggers
`.github/workflows/release.yml`, which builds all six targets, attaches every
platform binary to the GitHub release for that tag, then publishes the platform
packages and `free-pi-cli` to npm. The npm publish steps are skipped until an
`NPM_TOKEN` repository secret exists.

Because the `free-pi-cli` name on npm is already owned by the upstream
project, publishing this rewrite requires access to that package name (or
renaming the package and its optional dependencies consistently).

## License

MIT. Based on the original free-pi-cli by DappHero Corp / Dennison Bertram.
