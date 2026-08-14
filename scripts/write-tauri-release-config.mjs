import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

function requiredEnvironment(name) {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`Missing required environment variable: ${name}`);
  return value;
}

const releaseVersion = requiredEnvironment("RELEASE_VERSION").replace(/^v/u, "");
const runnerTemp = requiredEnvironment("RUNNER_TEMP");

if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/u.test(releaseVersion)) {
  throw new Error(`Invalid semantic version: ${releaseVersion}`);
}

const outputDirectory = join(runnerTemp, "deepharness-release");
const outputPath = join(outputDirectory, "tauri.release.conf.json");
mkdirSync(outputDirectory, { recursive: true });
writeFileSync(
  outputPath,
  `${JSON.stringify({ version: releaseVersion }, null, 2)}\n`,
  "utf8"
);

console.log(outputPath);
