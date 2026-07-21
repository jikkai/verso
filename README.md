# Verso

[English](README.md) | [简体中文](README.zh-CN.md)

[![CI](https://github.com/dream-num/verso/actions/workflows/ci.yml/badge.svg)](https://github.com/dream-num/verso/actions/workflows/ci.yml)
[![npm version](https://img.shields.io/npm/v/@univerkit/verso.svg)](https://www.npmjs.com/package/@univerkit/verso)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Verso is a small release CLI for JavaScript workspaces that publish multiple
packages at the same version. It updates package manifests, writes an
Angular-style conventional changelog, creates a release commit and tag, and
atomically pushes the current upstream branch and release tag.

## Installation

```sh
pnpm add -D @univerkit/verso
```

Add a release script to your package manifest:

```json
{
  "scripts": {
    "release": "verso"
  }
}
```

## Configuration

Every key is optional. `verso init` writes a starter config; without one,
Verso falls back to built-in defaults when a root `package.json` exists.

Most workspace projects only need `workspaces.patterns`. If you omit it,
Verso reads `pnpm-workspace.yaml` or the root manifest's `workspaces` field
before falling back to single-package mode.

```toml
[version]
root_package = "package.json"
require_consistent_versions = true
cargo_manifest_paths = []

[workspaces]
patterns = []
include_root = true
ignore = []
use_gitignore = true

[changelog]
infile = "CHANGELOG.md"
preset = "angular"

[git]
require_clean_worktree = true
commit_message = "chore(release): release v${version}"
tag_name = "v${version}"
push = "atomic"

[hooks]
# before_version = "pnpm test"
# after_version = "pnpm build"
# before_commit = "pnpm lint"
# after_push = "node scripts/notify-release.mts"

[github_release]
enabled = false
```

Explicit `--config <PATH>` values must always point at a real file. Package
discovery supports `package.json`, `package.json5`, `package.yaml`, and
`package.yml`; when several manifests sit in the same directory, Verso picks
in that order. The legacy `git.push = "follow-tags"` value is accepted as an
alias for the safer atomic mode.

### All keys, most-tuned to least-tuned

| Key | Default | Description |
| --- | --- | --- |
| `workspaces.patterns` | `[]` | Workspace globs relative to the config directory. Forward slashes. Supports `*`, `**`, `?`, character classes, braces, and `!` exclusions. When omitted, reads `pnpm-workspace.yaml` or root manifest `workspaces`; otherwise single-package mode. |
| `workspaces.include_root` | `true` | Include the package selected by `version.root_package`. |
| `workspaces.ignore` | `[]` | Extra ignore patterns during discovery. Plain path segments such as `fixtures` match directories by name. |
| `workspaces.use_gitignore` | `true` | Read root and nested `.gitignore` during discovery. Set `false` if a project intentionally publishes from ignored directories. |
| `version.root_package` | `package.json` | Manifest read for the current version and root update. Forward slashes; must stay under the config directory. If omitted and `package.json` is absent, Verso tries `package.json5`, `package.yaml`, then `package.yml`. |
| `version.require_consistent_versions` | `true` | Fail when discovered packages or configured Cargo manifests don't share one version. |
| `version.cargo_manifest_paths` | `[]` | Cargo manifests whose `[package].version` is updated. The nearest `Cargo.lock` is updated when present. |
| `changelog.infile` | `CHANGELOG.md` | Changelog file prepended during release. Forward slashes; must stay under the config directory. |
| `changelog.preset` | `angular` | Only `angular` is supported. |
| `git.require_clean_worktree` | `true` | Require a clean worktree before mutating files. When `false`, unrelated unstaged changes are allowed, but the index, release manifests, Cargo lockfiles, and the changelog must remain clean. |
| `git.commit_message` | `chore(release): release v${version}` | `${version}` is replaced with the target version. Must not be empty. |
| `git.tag_name` | `v${version}` | Must contain `${version}` and render a valid Git tag. |
| `git.push` | `atomic` | Atomically push the current upstream branch and exact release tag. Only `atomic` is supported. |
| `hooks.before_version` | None | Shell command run before release files are updated. |
| `hooks.after_version` | None | Shell command run after release files are updated. |
| `hooks.before_commit` | None | Shell command run before staging and committing. |
| `hooks.after_commit` | None | Shell command run after the release commit is created. |
| `hooks.before_tag` | None | Shell command run before the release tag is created. |
| `hooks.after_tag` | None | Shell command run after the release tag is created. |
| `hooks.before_push` | None | Shell command run before the atomic branch and tag push. |
| `hooks.after_push` | None | Shell command run after the push succeeds. |
| `github_release.enabled` | `false` | `true` is rejected in this version. |

## CLI

```sh
pnpm release
pnpm release -- --dry-run
pnpm release -- --version 0.26.0
pnpm release -- --version 0.26.0 --yes
pnpm release -- --dry-run --json
pnpm release -- --config path/to/verso.toml
pnpm release -- doctor
pnpm release -- init
pnpm release -- -V
pnpm release -- --help
```

| Option | Default | Description |
| --- | --- | --- |
| `--dry-run` | `false` | Preview the release without writing files or running mutating git commands. |
| `--json` | `false` | Print dry-run output as JSON. Must be used with `--dry-run`. |
| `--version <SEMVER>` | None | Use an exact target version without interactive selection. |
| `--config <PATH>` | `verso.toml` | Read a different config file. |
| `--yes` | `false` | Skip release confirmation prompts. It does not choose a version. |
| `-V, --tool-version` | None | Print the Verso CLI version. |
| `--help` | None | Print CLI help. |

Subcommands:

| Command | Description |
| --- | --- |
| `verso init` | Create a starter `verso.toml`. It auto-detects `packages/*`; use `--single`, `--workspace`, or `--force` to override behavior. |
| `verso doctor` | Validate config parsing, package discovery, version consistency, changelog path, Cargo manifest versions, and Git upstream readiness. Use `--json` for structured output. |

Without `--version`, Verso opens an interactive menu for patch, minor, major,
alpha, beta, rc, or custom semver. When the current version is a prerelease,
the menu also offers its stable version. Prerelease channels then prompt for a
base version choice, including a custom base version. Exact versions can be
passed with `--version`, including prereleases such as `0.26.0-alpha.0`,
`0.26.0-beta.1`, and `0.26.0-rc.2`.

Use `--config` to point at a different config file. Use `--yes` to skip release
confirmation prompts, including the default-no confirmation shown when a target
version is not greater than the current version. `--yes` does not choose a
version for you; without `--version`, interactive version selection still runs.
Use `-V` or `--tool-version` to print the installed Verso CLI version without
reading release config.

When stdin or stdout is not attached to a terminal, Verso keeps a plain text
prompt fallback so scripted tests and piped input can continue to choose by
name, such as `beta` followed by `minor`.

## What A Release Does

```text
            +----------------------------+
            |  Read verso.toml +         |
            |  discover manifests        |
            +-------------+--------------+
                          |
                          v
            +----------------------------+
            |  Resolve version           |
            |  (menu / --version)        |
            +-------------+--------------+
                          |
                          v
            +----------------------------+
            |  Write version files       |
            |  o manifests               |
            |  o Cargo.toml + Cargo.lock |
            |  o CHANGELOG.md prepend    |
            +-------------+--------------+
                          |
                          v
            +----------------------------+
            |  Commit                    |
            +-------------+--------------+
                          |
                          v
            +----------------------------+
            |  Tag                       |
            +-------------+--------------+
                          |
                          v
            +----------------------------+
            |  Atomic branch + tag push  |
            +----------------------------+

  -- [hooks] fire between steps
  -- --dry-run short-circuits every mutating step (no writes, no git)
  -- local failure: rollback files + unstage; push failure: keep commit/tag
```

Maintainer development and publishing details live in
[CONTRIBUTING.md](CONTRIBUTING.md).
