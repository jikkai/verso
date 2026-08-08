# Security policy

Verso edits release files, executes configured shell hooks, creates Git objects, and pushes refs.
Report vulnerabilities privately so a fix can be coordinated before public disclosure.

## Supported versions

Security fixes are shipped in the latest published `@amamo/verso` version. An active prerelease line
may receive a fix when it is the only affected line still under development.

## Trust model

Treat the repository and its release configuration as trusted input:

- Every configured hook is executed through `sh -c` on macOS/Linux or `cmd /C` on Windows.
- Dry-run text and JSON include hook commands verbatim. Never embed credentials in `verso.toml`;
  pass them through the environment and control where preview output is stored.
- Package manifests and Conventional Commit subjects are parsed and may be reproduced in diagnostics
  or changelog output. Do not store secrets in release metadata.
- The npm wrapper executes the binary supplied by the platform-specific optional package. Install
  `@amamo/verso` from the expected registry and retain the lockfile.

`--dry-run` prevents Verso's own writes, hooks, commits, tags, and pushes. It is a planning tool, not a
sandbox for untrusted repository content.

## Git and rollback limits

Atomic push means the remote accepts the branch and exact release tag together or accepts neither.
It does not make local hooks transactional, and it cannot undo an `after_push` failure.

Rollback before push is best-effort. Verso snapshots release files, unstages only its paths, uses a
soft reset for the expected release commit, and deletes its new tag where appropriate. If a hook
moves `HEAD` or a filesystem/Git cleanup fails, Verso reports partial cleanup rather than applying a
destructive reset. User cancellation keeps completed checkpoints by design.

## Reportable security issues

Examples include:

- executing an unexpected binary or command without an explicitly configured hook
- escaping the release root to read or write another path
- including unrelated staged work in a release commit
- pushing a ref other than the configured upstream branch and exact release tag
- leaking credentials through diagnostics, logs, release artifacts, or publication helpers
- substitution or misleading provenance of native binaries or npm packages
- a rollback operation that destroys pre-existing user work

Ordinary release bugs, unsupported platforms, and documentation mistakes may use public issues unless
they also create one of these risks.

## Private reporting

Email `白熱 <sonne@asaki.me>` with the subject prefix `[verso security]`. Do not open a public issue
for a suspected vulnerability.

Include the affected version, OS and CPU, Node.js and package-manager versions, a minimal reproduction,
the state before and after the command, and whether the issue can execute commands, change files or
refs, leak data, or publish artifacts.

We aim to acknowledge a report within five business days, investigate impact, and agree on a
disclosure timeline. Accepted reporters are credited unless they prefer otherwise.
