// Kept in sync with Cargo.toml by scripts/check-version.ts (and passed again
// through FREEPI_CLI_VERSION at launch, so the server gate always sees the
// binary's real version).
export const CLI_VERSION: string = process.env.FREEPI_CLI_VERSION ?? "0.7.0";
