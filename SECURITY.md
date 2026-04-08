# Security Policy

## Supported Versions

TermiFlow is currently in public beta (0.1.x). Security fixes are applied to
the latest release only.

## Reporting a Vulnerability

Please **do not** open a public GitHub issue for security vulnerabilities.

Report vulnerabilities privately via GitHub's
[Security Advisories](https://github.com/dnvt/termiflow/security/advisories/new)
or by emailing the maintainer directly (see GitHub profile for contact).

We aim to acknowledge reports within 72 hours and to provide a fix or mitigation
timeline within 14 days.

## Scope

TermiFlow reads Mermaid diagram files from the local filesystem and renders them
as text. It does not make network requests, store credentials, or execute
arbitrary code from diagram inputs. The primary attack surface is malformed input
causing a crash or unexpected output — please report any such cases.
