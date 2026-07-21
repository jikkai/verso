# Contributing

Thanks for taking the time to improve Verso.

## Local Setup

Requires Node.js 22.18.0+ and Rust 1.85+ (pinned via `rust-toolchain.toml`).

```sh
pnpm install
pnpm run check
```

`pnpm run check` is what CI runs on Linux, macOS, and Windows. It covers:

- `oxfmt --check` for TypeScript and `.mts` files
- `oxlint`
- `cargo fmt --check`
- `cargo clippy --locked --all-targets --all-features -- -D warnings`
- `cargo test --locked --all`
- TypeScript checks for the release helper scripts and the npm wrapper
- Wrapper tests

PRs that change dependency manifests or lockfiles run Dependency Review before
merge.

## Code Standards

Keep Rust formatted with rustfmt and free of clippy warnings. Keep TypeScript
and `.mts` files formatted with oxfmt, linted with oxlint, and strict-typecheck
clean. Add or update tests for behavior changes — release flow, rollback,
versioning, and package boundary changes in particular.

Prefer small, direct changes that match the existing structure. Verso is meant
to stay focused, so new behavior should have a clear release-workflow use case.

`.editorconfig` covers editor defaults. VS Code users should install the
recommended Oxc extension from `.vscode/extensions.json`; the workspace
settings format JavaScript and TypeScript files on save. `pnpm install`
installs a simple-git-hooks pre-commit hook that runs `pnpm run precommit`,
which checks oxfmt, rustfmt, and oxlint before a commit is created.

## Package Topology

The npm wrapper and native platform packages are intentionally kept in one
workspace. When adding or removing a supported platform, update these
together:

- `packages/verso/src/resolve.ts`
- `packages/verso/package.json`
- the matching `packages/verso-*` package manifest and README
- `.github/workflows/release.yml`
- `pnpm-workspace.yaml`

Keep `pnpm-workspace.yaml` `supportedArchitectures` in sync with the supported
platform packages so the optional dependency lockfile stays stable across
operating systems and CPU architectures.

## Commit And Changelog Style

The changelog is generated from Conventional Commits:

- `feat(scope): summary` — user-facing features
- `fix(scope): summary` — bug fixes
- `perf(scope): summary` — performance improvements

Use `type(scope)!: summary` or a `BREAKING CHANGE:` footer for breaking
changes. Other conventional commit types land in their own "Other Changes"
section. Non-conventional commits are ignored and may yield a `No classifiable
changes` entry when nothing else is releasable.

## Issues And Pull Requests

Use the bug report template for reproducible failures and the feature request
template for release-workflow proposals. For security concerns, follow
`SECURITY.md`. For conduct concerns, follow `CODE_OF_CONDUCT.md`.

PRs should include a short summary and the verification performed. For release
behavior changes, include dry-run output or tests covering the planned file,
changelog, git, and rollback effects.

## Publishing

Publishing is driven by GitHub Actions:

1. `Prepare Release` — run with a target version to update package versions,
   write the changelog, commit, tag, and push.
2. `Release` — the pushed `v*` tag triggers binary builds and npm publish.

The repository needs `GH_TOKEN` (repo contents read/write) and `NPM_TOKEN`
(publish access for the `@univerkit` scope). Trusted publishing is a planned
migration; keep `NPM_TOKEN` configured until that path is verified.

Stable versions publish with the `latest` dist-tag; `alpha`/`beta`/`rc`
publish with their matching tags so prereleases don't replace stable. The
same flagging is applied to GitHub Releases, and rerunning the workflow
reapplies the flags before replacing assets.

## Release Troubleshooting

If `Prepare Release` fails before creating the release commit or tag, fix the
reported check and rerun with the same version — no registry or tag cleanup
needed.

If it creates the commit and tag but can't push, inspect the action logs,
confirm the commit/tag match what you want, then push them together:

```sh
git push --atomic <remote> HEAD:<upstream-branch> \
  refs/tags/<release-tag>:refs/tags/<release-tag>
```

`Release` is safe to rerun for the same tag. Reruns refresh GitHub Release
notes and replace binary assets. `scripts/publish-npm-packages.mts` skips
already-published versions, so a partial npm publish can resume from the
remaining packages.

For binary asset issues, verify checksums first:

```sh
shasum -a 256 -c SHA256SUMS.txt
```

Then verify the binary provenance with GitHub Artifact Attestations:

```sh
gh attestation verify ./verso-linux-x64/verso \
  --repo dream-num/verso \
  --signer-workflow dream-num/verso/.github/workflows/release.yml
```
