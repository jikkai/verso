# @amamo/verso

The npm entry point for the [Verso](https://github.com/jikkai/verso) release CLI. It installs a
small JavaScript launcher plus the native package for the current platform.

## Supported targets

| Operating system | CPU   | Optional package            |
| ---------------- | ----- | --------------------------- |
| macOS            | arm64 | `@amamo/verso-darwin-arm64` |
| macOS            | x64   | `@amamo/verso-darwin-x64`   |
| Linux (GNU)      | arm64 | `@amamo/verso-linux-arm64`  |
| Linux (GNU)      | x64   | `@amamo/verso-linux-x64`    |
| Windows          | x64   | `@amamo/verso-win32-x64`    |

Node.js 22.18 or newer is required. Install the wrapper, not a platform package directly, with your
preferred package manager:

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

```sh
verso doctor
verso --dry-run --version 1.4.0
verso --version 1.4.0 --yes
```

A single package can use built-in defaults. A workspace usually needs only:

```toml
[workspaces]
patterns = ["packages/*"]
```

Verso updates one shared version, optionally writes a changelog, creates a release commit and
annotated tag, and atomically pushes the current upstream branch plus that exact tag. It does not
publish packages or create GitHub Releases.

Read the full [configuration](https://jikkai.github.io/verso/configuration/),
[CLI reference](https://jikkai.github.io/verso/cli-reference/), and
[release state model](https://jikkai.github.io/verso/release-workflow/).

## Launcher behavior

The wrapper resolves the matching optional package, adds a missing executable bit on macOS/Linux,
spawns the native binary with inherited stdio, and forwards its exit status or signal.

`-V` and `--tool-version` are handled before native package resolution when they are the only
argument, so the installed wrapper version can still be inspected if the optional package is missing.

## Troubleshooting

`Could not find Verso platform binary` means the platform optional dependency is unavailable or was
not installed. Confirm that optional dependencies are enabled, the target appears in the table, and
reinstall `@amamo/verso`.

`Failed to launch Verso binary` means the package was found but the executable could not start.
Reinstall, retain the printed cause, and include the OS, CPU, Node.js version, package-manager version,
and `verso --tool-version` output in a bug report.
