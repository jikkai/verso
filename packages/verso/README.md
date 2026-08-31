# @amamo/verso

The npm package for the [Verso](https://github.com/jikkai/verso) release CLI.

## Supported targets

| Operating system | CPU   |
| ---------------- | ----- |
| macOS            | arm64 |
| macOS            | x64   |
| Linux (GNU)      | arm64 |
| Linux (GNU)      | x64   |
| Windows          | x64   |

Node.js 22.18 or newer is required. Install Verso with your preferred package manager:

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

## First release

The following examples show Verso's CLI syntax. Invoke the local executable through your package
manager or an existing project task; the [getting-started guide](https://jikkai.github.io/verso/getting-started/)
shows the exact command for each supported package manager.

```sh
verso doctor
verso --dry-run --version 1.4.0
verso --version 1.4.0 --yes
```

A single package can use built-in defaults. Verso includes the root package by default. A workspace
whose root manifest should not share the released version needs an explicit package pattern:

```toml
[workspaces]
patterns = ["packages/*"]
include_root = false
```

Verso updates one shared version, optionally writes a changelog, creates a release commit and
annotated tag, and atomically pushes the current upstream branch plus that exact tag. It does not
publish packages or create GitHub Releases.

Read the full [configuration](https://jikkai.github.io/verso/configuration/),
[CLI reference](https://jikkai.github.io/verso/cli-reference/), and
[release state model](https://jikkai.github.io/verso/release-workflow/).

## Troubleshooting

`Could not find Verso platform binary` means the installation is incomplete or the platform is not
supported. Confirm that optional dependencies are enabled, check the table above, and reinstall
`@amamo/verso`.

`Failed to launch Verso binary` means the installed executable could not start. Reinstall, retain the
printed cause, and include the OS, CPU, Node.js version, package-manager version, and
`verso --tool-version` output in a bug report.
