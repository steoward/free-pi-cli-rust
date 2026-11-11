#!/usr/bin/env bun
// free-pi-cli's npm entry point. The real CLI is a native Rust binary shipped
// in a per-platform optional dependency; this shim finds it, points it at the
// pi package installed alongside this shim, and hands over the terminal.
import { spawnSync } from "node:child_process";
import { chmodSync, existsSync, realpathSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const here = dirname(fileURLToPath(import.meta.url));
const packageRoot = resolve(here, "..");

const platform = process.platform;
const arch = process.arch;
const exeName = platform === "win32" ? "free-pi-cli.exe" : "free-pi-cli";
const platformPackage = `free-pi-cli-${platform}-${arch}`;
const supported = [
  "win32-x64",
  "win32-arm64",
  "darwin-x64",
  "darwin-arm64",
  "linux-x64",
  "linux-arm64",
];

function fromEnv(name) {
  const value = process.env[name];
  if (!value) return null;
  const path = isAbsolute(value) ? value : resolve(process.cwd(), value);
  return existsSync(path) ? path : null;
}

function normalisePath(path) {
  let real = path;
  try {
    real = realpathSync(path);
  } catch {
    // A path that no longer exists still compares fine as text.
  }
  return real.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
}

// Bunx installs into a temporary `bunx-<id>-<package>` directory. The
// extension uses FREEPI_LAUNCHED_BY_BUNX to decide whether /update should
// explain that bunx already fetches the latest, or actually run
// `bun add -g free-pi-cli@latest` for a global install.
function isBunxRun() {
  const root = normalisePath(packageRoot);
  const tempRoots = new Set(
    [tmpdir(), process.env.TEMP, process.env.TMP, process.env.TMPDIR]
      .filter(Boolean)
      .map(normalisePath),
  );
  for (const temp of tempRoots) {
    if (root.startsWith(`${temp}/bunx-`)) return true;
  }
  return /\/bunx-[^/]+\//.test(root);
}

function collectAncestorCandidates(relative) {
  const candidates = [];
  let dir = here;
  for (;;) {
    candidates.push(join(dir, "node_modules", relative));
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return candidates;
}

function findNativeBinary() {
  const override = fromEnv("FREEPI_CLI_BIN");
  if (override) return override;

  // The normal published-install path: the optional platform package was
  // installed next to this package by npm/bun.
  try {
    const packageJson = require.resolve(`${platformPackage}/package.json`);
    const candidate = join(dirname(packageJson), "bin", exeName);
    if (existsSync(candidate)) return candidate;
  } catch {
    // Optional dependencies can be omitted (--no-optional); fall through.
  }

  for (const candidate of collectAncestorCandidates(join(platformPackage, "bin", exeName))) {
    if (existsSync(candidate)) return candidate;
  }

  // Running from a checkout or from a package assembled by `bun run
  // package:npm` before the platform package was installed.
  const local = [
    join(packageRoot, "target", "release", exeName),
    join(packageRoot, "target", "debug", exeName),
    join(packageRoot, "npm", platformPackage, "bin", exeName),
    join(process.cwd(), "target", "release", exeName),
    join(process.cwd(), "target", "debug", exeName),
  ];
  for (const candidate of local) {
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

function findPiCli() {
  const override = fromEnv("FREEPI_PI_CLI");
  if (override) return override;

  // `@earendil-works/pi-coding-agent` only exports its package entry, so
  // resolve that and walk to dist/bundle/cli.js instead of importing a
  // non-exported subpath.
  try {
    const entry = require.resolve("@earendil-works/pi-coding-agent");
    const candidate = join(dirname(entry), "bundle", "cli.js");
    if (existsSync(candidate)) return candidate;
  } catch {
    // The Rust CLI can still fall back to `bun x pi@...`.
  }

  const relative = join(
    "@earendil-works",
    "pi-coding-agent",
    "dist",
    "bundle",
    "cli.js",
  );
  for (const candidate of collectAncestorCandidates(relative)) {
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

const binary = findNativeBinary();
if (!binary) {
  const detail = supported.includes(`${platform}-${arch}`)
    ? `The ${platformPackage} package is missing. Reinstall free-pi-cli with optional dependencies enabled.`
    : `free-pi-cli does not ship a binary for ${platform}-${arch}. Supported: ${supported.join(", ")}.`;
  console.error(`free-pi-cli: ${detail}`);
  process.exit(1);
}

if (platform !== "win32") {
  try {
    chmodSync(binary, 0o755);
  } catch {
    // A read-only global store may reject chmod; the file is usually already
    // executable. Let the spawn below report a real failure.
  }
}

const env = { ...process.env };
if (isBunxRun()) {
  env.FREEPI_LAUNCHED_BY_BUNX = "1";
}
if (process.versions.bun) {
  // bunx may have been started from a path that is not on PATH. Pin the Rust
  // launcher to the exact Bun that is running this shim.
  env.FREEPI_JS_RUNTIME ??= process.execPath;
}
const piCli = findPiCli();
if (piCli) {
  env.FREEPI_PI_CLI ??= piCli;
}

const result = spawnSync(binary, process.argv.slice(2), {
  stdio: "inherit",
  env,
  windowsHide: false,
});

if (result.error) {
  console.error(`free-pi-cli: could not start the native CLI: ${result.error.message}`);
  process.exit(1);
}
if (result.signal === "SIGINT") process.exit(130);
if (result.signal === "SIGTERM") process.exit(143);
process.exit(result.status ?? 1);
