# Verso

[English](README.md) | [简体中文](README.zh-CN.md)

[![CI](https://github.com/jikkai/verso/actions/workflows/ci.yml/badge.svg)](https://github.com/jikkai/verso/actions/workflows/ci.yml)
[![npm version](https://img.shields.io/npm/v/@amamo/verso.svg)](https://www.npmjs.com/package/@amamo/verso)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Verso releases a JavaScript workspace as one version. It discovers package manifests, optionally
updates configured Cargo packages and a Conventional Commit changelog, creates a release commit and
annotated tag, then atomically pushes the current upstream branch and that exact tag.

Verso deliberately stops there. Registry publication, GitHub Releases, binary builds, and deployment
belong in tag-triggered CI.

## Requirements

- Node.js 22.18 or newer for the npm wrapper.
- Git, a named branch, and a configured upstream.
- One supported native target: macOS arm64/x64, Linux GNU arm64/x64, or Windows x64.
- A `package.json`, `package.json5`, `package.yaml`, or `package.yml` with a valid SemVer version.

## Quick start

```sh
pnpm add -D @amamo/verso
```

```json
{
  "scripts": {
    "release": "verso"
  }
}
```

A single-package repository needs no config file. For a workspace, create `verso.toml`:

```toml
[workspaces]
patterns = ["packages/*"]
```

Then inspect the repository and an exact release plan:

```sh
pnpm release -- doctor
pnpm release -- --dry-run --version 1.4.0
```

When the plan is correct, run interactively or provide an exact version for automation:

```sh
pnpm release
pnpm release -- --version 1.4.0 --yes
```

`--yes` accepts confirmations; it does not choose a version.

## Release model

```text
config + manifests + Git history
  -> validate project and shared versions
  -> resolve one target SemVer
  -> update package/Cargo versions and optional changelog
  -> commit -> annotated tag
  -> git push --atomic <upstream-branch> <exact-tag>
```

- `verso doctor` checks config, package discovery, versions, changelog path, Cargo packages, and the
  branch upstream without starting a release.
- `verso --dry-run` prints version files, hooks, warnings, and Git commands without writes or
  mutating Git commands.
- Real releases require a clean worktree by default. Relaxed mode still requires a clean index and
  clean release files.
- Execution failures before push use stage-aware local cleanup. User cancellation keeps completed
  checkpoints visible: modified files, a release commit, or a local tag may remain depending on the
  prompt.
- A push failure keeps the local commit and tag. An `after_push` hook failure occurs after the remote
  has accepted the release refs.

See the [release workflow](https://jikkai.github.io/verso/release-workflow/) for the complete state
matrix.

## Configuration

Every key is optional. `verso init` writes a starter, and `--config <PATH>` makes the containing
directory the release root. Unknown keys are rejected; config paths must be relative, use forward
slashes, and stay inside that root.

```toml
[version]
root_package = "package.json"
require_consistent_versions = true
cargo_manifest_paths = ["crates/cli/Cargo.toml"]

[workspaces]
patterns = ["packages/*", "!packages/fixtures"]
include_root = true
ignore = ["examples"]
use_gitignore = true

[changelog]
enabled = true
infile = "CHANGELOG.md"
preset = "angular"

[git]
require_clean_worktree = true
commit_message = "chore(release): release v${version}"
tag_name = "v${version}"
push = "atomic"

[hooks]
before_version = "pnpm test"
before_push = "pnpm run check"
```

Empty workspace patterns are inferred from `pnpm-workspace.yaml`, then the root manifest's
`workspaces` field. The first package manifest found in each matched directory wins in this order:
JSON, JSON5, `package.yaml`, `package.yml`.

Hooks are trusted shell commands and are included verbatim in dry-run output. Pass secrets through
the environment instead of embedding them in `verso.toml`.

Full details: [configuration](https://jikkai.github.io/verso/configuration/) and
[CLI reference](https://jikkai.github.io/verso/cli-reference/).

## Scope

Verso fits repositories where all releasable packages share one version and tag. It does not support
independent package versions, non-atomic push modes, local registry publishing, or
`github_release.enabled = true`.

Maintainer setup and publishing are in [CONTRIBUTING.md](CONTRIBUTING.md). Security reports follow
[SECURITY.md](SECURITY.md).

## License

MIT
