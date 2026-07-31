import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";

import type { IPlatformBinary } from "./resolve";
import {
  ensureExecutable,
  formatLaunchError,
  isToolVersionRequest,
  normalizeCliArgs,
} from "./launcher";
import { resolvePlatformBinary } from "./resolve";

const require = createRequire(import.meta.url);

function resolveInstalledBinaryPath({ packageName, binaryName }: IPlatformBinary): string {
  try {
    return require.resolve(`${packageName}/bin/${binaryName}`);
  } catch (cause) {
    throw new Error(
      `Could not find Verso platform binary ${packageName}/bin/${binaryName}. ` +
        `The optional dependency ${packageName} may not be installed for this platform.`,
      { cause },
    );
  }
}

function wrapperVersion(): string {
  const manifest = require("../package.json") as { version?: unknown };
  if (typeof manifest.version !== "string") {
    throw new Error("Could not read @amamo/verso package version.");
  }

  return manifest.version;
}

function main(): never {
  const args = process.argv.slice(2);
  if (isToolVersionRequest(args)) {
    console.log(wrapperVersion());
    process.exit(0);
  }

  const platformBinary = resolvePlatformBinary();
  const binaryPath = resolveInstalledBinaryPath(platformBinary);

  ensureExecutable(binaryPath);

  const result = spawnSync(binaryPath, normalizeCliArgs(args), {
    stdio: "inherit",
  });

  if (result.error !== undefined) {
    throw new Error(formatLaunchError(binaryPath, result.error));
  }

  if (result.signal !== null) {
    process.kill(process.pid, result.signal);
  }

  process.exit(result.status ?? 1);
}

try {
  main();
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  console.error(`error: ${message}`);
  if (error instanceof Error && error.cause instanceof Error) {
    console.error(`note: ${error.cause.message}`);
  }
  process.exit(1);
}
