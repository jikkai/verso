# @amamo/verso

The JavaScript wrapper for the [Verso](https://github.com/jikkai/verso)
release CLI. The native binary ships through an optional platform package; see
[Supported Platforms](#supported-platforms).

## Supported Platforms

| Platform | CPU   | Package                     |
| -------- | ----- | --------------------------- |
| macOS    | arm64 | `@amamo/verso-darwin-arm64` |
| macOS    | x64   | `@amamo/verso-darwin-x64`   |
| Linux    | arm64 | `@amamo/verso-linux-arm64`  |
| Linux    | x64   | `@amamo/verso-linux-x64`    |
| Windows  | x64   | `@amamo/verso-win32-x64`    |

## Installation

```sh
pnpm add -D @amamo/verso
```

Add a release script:

```json
{
  "scripts": {
    "release": "verso"
  }
}
```

## Usage

```sh
pnpm release                       # interactive version selection
pnpm release -- --dry-run          # preview without writing
pnpm release -- --version 1.2.3    # explicit version
pnpm release -- --version 1.2.3 --yes
pnpm release -- --dry-run --json   # JSON for CI / scripts
pnpm release -- init               # write a starter verso.toml
pnpm release -- doctor             # validate config + packages
pnpm release -- -V                 # print wrapper version
```

Without `--version`, Verso opens an interactive terminal menu. In
non-terminal environments it keeps a plain text fallback for scripts and
tests.

A typical workspace project only needs `verso.toml` for the workspace globs:

```toml
[workspaces]
patterns = ["packages/*"]
```

Without `verso.toml`, Verso falls back to built-in defaults when a root
`package.json` exists. For the full configuration reference, see the
[repository README](https://github.com/jikkai/verso#configuration).

## Troubleshooting

`Could not find Verso platform binary` — the native optional dependency for
your OS wasn't installed or isn't available. Check:

- install `@amamo/verso`, not a platform package directly
- optional dependencies are enabled in your package manager
- your machine is one of the supported platform/CPU pairs above
- reinstall from a fresh lockfile if the lockfile was made on a different OS
  or with optional dependencies disabled

`Failed to launch Verso binary` — the platform package was found but the
executable couldn't start. On macOS and Linux the wrapper repairs missing
executable bits before spawning the binary. Upgrade `@amamo/verso`,
reinstall, and include the error's cause line in bug reports if it persists.

For unsupported platforms, a new native package is needed before the wrapper
can run. Include `pnpm release -- -V` output in bug reports — the wrapper
handles it before loading the native binary, so it still works when the
optional dependency is missing.
