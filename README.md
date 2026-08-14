# Verso

[English](README.md) | [简体中文](README.zh-CN.md)

[![CI](https://github.com/jikkai/verso/actions/workflows/ci.yml/badge.svg)](https://github.com/jikkai/verso/actions/workflows/ci.yml)
[![npm version](https://img.shields.io/npm/v/@amamo/verso.svg)](https://www.npmjs.com/package/@amamo/verso)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Verso releases each configured JavaScript workspace group as one version. It discovers package
manifests, optionally updates configured Cargo packages and a Conventional Commit changelog, creates
a release commit and annotated tag, then atomically pushes the current upstream branch and that exact
tag.

Verso deliberately stops there. Registry publication, GitHub Releases, binary builds, and deployment
belong in tag-triggered CI.

## Requirements

- Node.js 22.18 or newer for the npm wrapper.
- Git, a named branch, and a configured upstream.
- One supported native target: macOS arm64/x64, Linux GNU arm64/x64, or Windows x64.
- A `package.json`, `package.json5`, `package.yaml`, or `package.yml` with a valid SemVer version.

## Quick start

Install `@amamo/verso` with your preferred package manager:

```sh
# npm
npm install --save-dev @amamo/verso
# pnpm
pnpm add --save-dev @amamo/verso
# Yarn
yarn add --dev @amamo/verso
# Bun
bun add --dev @amamo/verso
```

A single-package repository needs no config file. For a workspace, create `verso.toml`:

```toml
[workspaces]
patterns = ["packages/*"]
```

Then inspect the repository and an exact release plan:

```sh
verso doctor
verso --dry-run --version 1.4.0
```

Dry-run computes the same transformations as execution and prints the actual before/after diff for
every changed file, without writing it.

When the plan is correct, run interactively or provide an exact version for automation:

```sh
verso
verso --version 1.4.0 --yes
```

`--yes` accepts confirmations; it does not choose a version.

To prepare a version-only change for a release PR, use `bump`:

```sh
verso bump minor
verso bump --version 1.4.0
```

`bump` updates package/Cargo manifests and matching Cargo lock entries. It does not update the
changelog or create a commit, tag, or push.

## Release model

```text
config + manifests + Git history
  -> validate one release group and resolve one target SemVer
  -> calculate exact before/after file changes
  -> persist transaction -> update files -> commit -> annotated tag
  -> git push --atomic <upstream-branch> <exact-tag> -> clear transaction
```

- `verso doctor` checks config, package discovery, versions, changelog path, Cargo packages, and the
  branch upstream without starting a release.
- `verso --dry-run` prints the exact before/after file diff, hooks, warnings, and Git commands without
  writes or mutating Git commands.
- `verso bump patch|minor|major` or `verso bump --version <SEMVER>` applies only version-file changes.
- Real releases require a clean worktree by default. Relaxed mode still requires a clean index and
  clean release files.
- `verso status`, `verso resume`, and `verso abort` inspect, continue, or safely roll back an
  interrupted transaction. Once the release was pushed, it cannot be aborted; resume finishes any
  remaining `after_push` work.
- If a hook was interrupted, inspect its side effects and choose `verso resume --retry-hook` or
  `verso resume --skip-hook`. Once a push has started, automatic abort is disabled because the remote
  outcome may be unknown; resume verifies the exact remote tag object and release commit before
  finishing or retrying, and requires the remote branch to equal or contain that commit.

See the [release workflow](https://jikkai.github.io/verso/release-workflow/) for the complete state
matrix.

## Configuration

Every key is optional. `verso init` writes a starter, and `--config <PATH>` makes the containing
directory the release root. One config defines one release group; `--group core` selects
`verso.core.toml`. Use separate configs for independently versioned groups. Versions within each
group must remain consistent. Unknown keys are rejected; config paths must be relative, use forward
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

`changelog.preset` accepts `angular` and `keep-a-changelog`. Changelog generation belongs to a full
release; `bump` leaves it unchanged.

Full details: [configuration](https://jikkai.github.io/verso/configuration/) and
[CLI reference](https://jikkai.github.io/verso/cli-reference/).

## Scope

Verso keeps one consistent version and tag per configured release group. Independently versioned
groups use separate configs and are released one at a time; independent versions within one group
are not supported. Named groups using the default tag template automatically get tags such as
`core-v1.2.3`, avoiding collisions with other groups. Verso also does not support non-atomic push
modes, local registry publishing, or `github_release.enabled = true`.

Maintainer setup and publishing are in [CONTRIBUTING.md](CONTRIBUTING.md). Security reports follow
[SECURITY.md](SECURITY.md).

## License

MIT
