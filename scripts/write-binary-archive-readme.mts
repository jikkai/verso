import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";

type PlatformBinary = {
  path: string;
  label: string;
};

const platformBinaries: PlatformBinary[] = [
  { path: "verso-darwin-arm64/verso", label: "macOS arm64" },
  { path: "verso-darwin-x64/verso", label: "macOS x64" },
  { path: "verso-linux-arm64/verso", label: "Linux arm64" },
  { path: "verso-linux-x64/verso", label: "Linux x64" },
  { path: "verso-win32-x64/verso.exe", label: "Windows x64" },
];

function fail(message: string): never {
  console.error(message);
  process.exit(1);
}

function renderReadme(tag: string): string {
  const binaryLines = platformBinaries.map((binary) => `- \`${binary.path}\` - ${binary.label}`);

  return [
    "# Verso binary archive",
    "",
    `Release: \`${tag}\``,
    "",
    "This archive contains native Verso binaries built by the GitHub Release workflow.",
    "Most users should install and run `@amamo/verso` from npm instead.",
    "",
    "```sh",
    "pnpm add -D @amamo/verso",
    "```",
    "",
    "## Contents",
    "",
    ...binaryLines,
    "- `SHA256SUMS.txt` - SHA-256 checksums for the binaries",
    "- `LICENSE` - project license",
    "",
    "## Verify",
    "",
    "After extracting the archive, verify the binaries with:",
    "",
    "```sh",
    "shasum -a 256 -c SHA256SUMS.txt",
    "```",
    "",
    "When online, verify GitHub Artifact Attestations for a binary with:",
    "",
    "```sh",
    "gh attestation verify ./verso-linux-x64/verso \\",
    "  --repo jikkai/verso \\",
    "  --signer-workflow jikkai/verso/.github/workflows/release.yml",
    "```",
    "",
    "## Run",
    "",
    "Use the binary for your platform, for example:",
    "",
    "```sh",
    "./verso-linux-x64/verso --help",
    "```",
    "",
  ].join("\n");
}

const [, , tag, outputPath] = process.argv;
if (tag === undefined || outputPath === undefined) {
  fail("Usage: node scripts/write-binary-archive-readme.mts <tag> <output-path>");
}

await mkdir(dirname(resolve(outputPath)), { recursive: true });
await writeFile(outputPath, renderReadme(tag));
console.log(`Wrote binary archive README to ${outputPath}`);
