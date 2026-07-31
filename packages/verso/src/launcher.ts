import { chmodSync, statSync } from "node:fs";

export function normalizeCliArgs(args: string[]): string[] {
  return args[0] === "--" ? args.slice(1) : args;
}

export function isToolVersionRequest(args: string[]): boolean {
  const normalizedArgs = normalizeCliArgs(args);
  return (
    normalizedArgs.length === 1 &&
    (normalizedArgs[0] === "-V" || normalizedArgs[0] === "--tool-version")
  );
}

export function hasExecutableBit(mode: number): boolean {
  return (mode & 0o111) !== 0;
}

export function ensureExecutable(
  binaryPath: string,
  platform: NodeJS.Platform = process.platform,
): void {
  if (platform === "win32") {
    return;
  }

  const mode = statSync(binaryPath).mode;
  if (hasExecutableBit(mode)) {
    return;
  }

  chmodSync(binaryPath, mode | 0o111);
}

export function formatLaunchError(binaryPath: string, error: Error): string {
  const lines = [`Failed to launch Verso binary at ${binaryPath}.`];
  const reason = error.message.trim();
  if (reason.length > 0) {
    lines.push(`note: ${reason}`);
  }

  if ((error as NodeJS.ErrnoException).code === "EACCES") {
    lines.push(
      "",
      "help: reinstall @amamo/verso, then check that the package manager installs optional dependencies and preserves executable file modes.",
    );
  }

  return lines.join("\n");
}
