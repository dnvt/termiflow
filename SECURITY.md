# Security Policy

## Supported Versions

The latest tagged release is v0.1.1, and the supported release line is 0.1.x.
Security fixes are applied to the latest released 0.1.x version. The current
repository source targets version 0.2.0 but is an unreleased candidate; when
reporting an issue against it, include the commit SHA as well as the affected
TermiFlow version and toolchain.

## Reporting a Vulnerability

Please **do not** open a public GitHub issue for security vulnerabilities.

Report vulnerabilities privately via GitHub's
[Security Advisories](https://github.com/dnvt/termiflow/security/advisories/new)
or by emailing the maintainer directly (see GitHub profile for contact).

We aim to acknowledge reports within 72 hours and to provide a fix or mitigation
timeline within 14 days.

Please do not include secrets, private diagrams, credentials, or other
sensitive inputs in public issues. If a reproducer contains sensitive data,
attach a minimized private example through the advisory process.

## Scope

TermiFlow reads Mermaid diagram files from the local filesystem and renders them
as text. It does not make network requests, store credentials, or execute
arbitrary code from diagram inputs. The primary attack surface is malformed input
causing a crash or unexpected output — please report any such cases.
