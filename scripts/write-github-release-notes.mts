import { mkdir, writeFile } from "node:fs/promises";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

function fail(message: string): never {
  console.error(message);
  process.exit(1);
}

function headingMatchesVersion(line: string, version: string): boolean {
  if (!line.startsWith("## ")) {
    return false;
  }

  const heading = line.replace(/^##\s+/, "").trim();
  return (
    heading === version || heading.startsWith(`${version} `) || heading.startsWith(`[${version}]`)
  );
}

function releaseNotesForTag(changelog: string, tag: string): string {
  const version = tag.replace(/^v/, "");
  const lines = changelog.split(/\r?\n/);
  const headingIndex = lines.findIndex((line) => headingMatchesVersion(line, version));

  const fallbackNotes = `Release ${tag}\n`;
  const changelogNotes = (() => {
    if (headingIndex === -1) {
      return fallbackNotes;
    }

    const bodyStart = headingIndex + 1;
    const nextHeadingIndex = lines.findIndex(
      (line, index) => index > headingIndex && line.startsWith("## "),
    );
    const bodyEnd = nextHeadingIndex === -1 ? lines.length : nextHeadingIndex;
    const body = lines.slice(bodyStart, bodyEnd).join("\n").trim();

    return body.length > 0 ? `${body}\n` : fallbackNotes;
  })();

  return [
    changelogNotes.trimEnd(),
    "",
    "## Binary Assets",
    "",
    "The GitHub Release includes `verso-binaries.tar.gz` and `SHA256SUMS.txt`.",
    "After extracting the archive, verify the binaries with:",
    "",
    "```sh",
    "shasum -a 256 -c SHA256SUMS.txt",
    "```",
    "",
    "GitHub Artifact Attestations are generated for the native binaries. When online, verify one with:",
    "",
    "```sh",
    "gh attestation verify ./verso-linux-x64/verso \\",
    "  --repo jikkai/verso \\",
    "  --signer-workflow jikkai/verso/.github/workflows/release.yml",
    "```",
    "",
  ].join("\n");
}

const [, , tag, outputPath] = process.argv;
if (tag === undefined || outputPath === undefined) {
  fail("Usage: node scripts/write-github-release-notes.mts <tag> <output-path>");
}

const notes = releaseNotesForTag(readFileSync("CHANGELOG.md", "utf8"), tag);
await mkdir(dirname(resolve(outputPath)), { recursive: true });
await writeFile(outputPath, notes);
console.log(`Wrote GitHub release notes to ${outputPath}`);
