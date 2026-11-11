// Fails fast when the release metadata drifts: Cargo, the root npm package,
// every platform package, the pinned pi dependency, and the tag must agree.
import { existsSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const errors = [];

function read(relative) {
  return readFileSync(join(root, relative), "utf8");
}

const rootPackage = JSON.parse(read("package.json"));
const cargo = read("Cargo.toml");
const cargoVersion = /^version\s*=\s*"([^"]+)"/m.exec(cargo)?.[1];
if (!cargoVersion) errors.push("could not read the version from Cargo.toml");
if (cargoVersion && cargoVersion !== rootPackage.version) {
  errors.push(`Cargo.toml is ${cargoVersion}, package.json is ${rootPackage.version}`);
}

const pinnedPi = /PINNED_PI_VERSION:\s*&str\s*=\s*"([^"]+)"/.exec(
  read("src/version.rs"),
)?.[1];
const dependencyPi = rootPackage.dependencies?.["@earendil-works/pi-coding-agent"];
if (!pinnedPi) errors.push("could not read PINNED_PI_VERSION from src/version.rs");
if (pinnedPi && dependencyPi !== pinnedPi) {
  errors.push(
    `src/version.rs pins pi ${pinnedPi}, but package.json depends on ${dependencyPi}`,
  );
}

const platforms = [
  "win32-x64",
  "win32-arm64",
  "darwin-x64",
  "darwin-arm64",
  "linux-x64",
  "linux-arm64",
];
for (const platform of platforms) {
  const name = `free-pi-cli-${platform}`;
  const expected = rootPackage.optionalDependencies?.[name];
  if (expected !== rootPackage.version) {
    errors.push(`optionalDependencies.${name} must be ${rootPackage.version} (got ${expected})`);
  }
  const path = join(root, "npm", name, "package.json");
  if (!existsSync(path)) {
    errors.push(`missing ${path}`);
    continue;
  }
  const pkg = JSON.parse(readFileSync(path, "utf8"));
  if (pkg.name !== name) errors.push(`${path} is named ${pkg.name}`);
  if (pkg.version !== rootPackage.version) {
    errors.push(`${name} is ${pkg.version}, expected ${rootPackage.version}`);
  }
}

const extension = join(root, "assets", "free-pi-extension.js");
if (!existsSync(extension) || statSync(extension).size < 100_000) {
  errors.push("assets/free-pi-extension.js is missing or looks unbuilt");
}

const tagIndex = process.argv.indexOf("--tag");
if (tagIndex >= 0) {
  const tag = process.argv[tagIndex + 1];
  if (tag !== `v${rootPackage.version}`) {
    errors.push(`tag ${tag} does not match v${rootPackage.version}`);
  }
}

if (errors.length > 0) {
  console.error(`release verification failed:\n- ${errors.join("\n- ")}`);
  process.exit(1);
}
console.log(`release metadata OK (free-pi-cli ${rootPackage.version}, pi ${pinnedPi})`);
