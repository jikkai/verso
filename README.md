<p align="center">
  <img src="brand/verso-readme-banner.svg" alt="Verso" width="800" />
</p>

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

- Node.js 22.18 or newer.
- Git, a named branch, and a configured upstream.
- A supported platform: macOS arm64/x64, Linux GNU arm64/x64, or Windows x64.
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

The examples below show Verso's CLI syntax. Invoke the local executable through your package manager
or an existing project task; the [getting-started guide](https://jikkai.github.io/verso/getting-started/)
shows the exact command for each supported package manager.

A single-package repository needs no config file. Verso includes the root package by default. For a
workspace whose root manifest should not share the released version, create `verso.toml` with an
explicit package pattern:

```toml
[workspaces]
patterns = ["packages/*"]
include_root = false
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
  interrupted transaction. Once push starts, abort is disabled and recovery continues through
  `resume`.

See the [release workflow](https://jikkai.github.io/verso/release-workflow/) for the complete state
matrix.

## Configuration

Every key is optional. `verso init` writes a starter. `--config <PATH>` accepts a relative or absolute
file path and makes its directory the release root; paths inside the file must stay relative to that
root. One config defines one release group, and `--group core` selects `verso.core.toml`. Use separate
configs for independently versioned groups.

See the complete [configuration reference](https://jikkai.github.io/verso/configuration/) and
[CLI reference](https://jikkai.github.io/verso/cli-reference/).

## Scope

Verso keeps one consistent version and tag per configured release group. Independently versioned
groups use separate configs and are released one at a time; independent versions within one group
are not supported. Named groups using the default tag template automatically get tags such as
`core-v1.2.3`, avoiding collisions with other groups. Verso also does not support non-atomic push
modes, local registry publishing, or local GitHub Release creation.

Maintainer setup and publishing are in [CONTRIBUTING.md](CONTRIBUTING.md). Security reports follow
[SECURITY.md](SECURITY.md).

## License

MIT
