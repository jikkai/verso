# Security

Verso automates local release steps, edits package manifests, writes changelogs,
and runs git commands. Please report security concerns privately before opening
a public issue.

## Supported Versions

Security fixes are shipped in the latest published version of
`@amamo/verso`. Pre-release builds may receive fixes when they are the
active release line.

## Security Scope

Verso is a release automation tool. Please treat these issues as security
concerns:

- arbitrary command execution through release configuration, changelog content,
  package metadata, or workspace discovery
- path traversal or writes outside the configured project directory
- behavior that exposes npm tokens, GitHub tokens, or other release credentials
- tampering, substitution, or misleading provenance for release artifacts
- npm wrapper or platform package behavior that launches an unexpected binary

General release bugs, unsupported platforms, missing changelog entries, and
documentation mistakes can use the public issue templates unless they also
create one of the risks above.

## Reporting A Vulnerability

Email `白熱 <sonne@asaki.me>` with the subject prefix `[verso security]`.
Do not open a public issue for suspected vulnerabilities.

Please include:

- the affected Verso version
- the operating system and package manager version
- a minimal reproduction or command transcript
- whether the issue can modify files, run commands, leak data, or publish tags
- whether the issue affects published npm packages, GitHub Release assets, or
  generated provenance

## Coordinated Disclosure

We aim to acknowledge reports within five business days, investigate the impact,
and coordinate a fix before sharing details publicly. If the report is accepted,
we will agree on a disclosure timeline with the reporter and credit them unless
they prefer to stay anonymous.
