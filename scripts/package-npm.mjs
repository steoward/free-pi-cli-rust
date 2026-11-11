// Copies a compiled Rust binary into its npm platform package.
// CI runs this once per target, e.g.:
//   node scripts/package-npm.mjs --platform win32-x64 \
//     --binary target/x86_64-pc-windows-msvc/release/free-pi-cli.exe
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);

function option(name) {
  const index = args.indexOf(`--${name}`);
  return index >= 0 ? args[index + 1] : undefined;
}

const supported = [
  "win32-x64",
  "win32-arm64",
  "darwin-x64",
  "darwin-arm64",
  "linux-x64",
  "linux-arm64",
];

const hostKey = `${process.platform}-${process.arch}`;
const platformKey = option("platform") ?? hostKey;
if (!supported.includes(platformKey)) {
  console.error(
    `package-npm: unsupported platform "${platformKey}" (expected one of ${supported.join(", ")})`,
  );
  process.exit(1);
}

const exeName = platformKey.startsWith("win32-") ? "free-pi-cli.exe" : "free-pi-cli";
const requestedBinary = option("binary");
const candidates = requestedBinary
  ? [resolve(root, requestedBinary)]
  : [
      join(root, "target", "release", exeName),
      join(root, "target", "debug", exeName),
    ];
const binary = candidates.find((candidate) => existsSync(candidate));
if (!binary) {
  console.error(
    `package-npm: binary not found. Build it first or pass --binary <path>. Tried:\n  ${candidates.join("\n  ")}`,
  );
  process.exit(1);
}

const packageDir = join(root, "npm", `free-pi-cli-${platformKey}`);
const packageJsonPath = join(packageDir, "package.json");
const rootPackage = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const platformPackage = JSON.parse(readFileSync(packageJsonPath, "utf8"));
if (platformPackage.version !== rootPackage.version) {
  console.error(
    `package-npm: ${platformPackage.name} is ${platformPackage.version}, but free-pi-cli is ${rootPackage.version}`,
  );
  process.exit(1);
}

const destination = join(packageDir, "bin", exeName);
mkdirSync(dirname(destination), { recursive: true });
copyFileSync(binary, destination);
if (!platformKey.startsWith("win32-")) {
  chmodSync(destination, 0o755);
}

console.log(
  `packaged ${platformPackage.name}@${platformPackage.version}: ${destination}`,
);
