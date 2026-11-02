# Security

## Reporting a vulnerability

Please do not open a public issue. Report it privately through GitHub: on the [Security tab](https://github.com/sionic-ai/openpaper/security/advisories/new) of this repository, choose "Report a vulnerability". Include the version, steps to reproduce, and the impact. We will reply as soon as we can and credit you in the fix unless you prefer otherwise.

Only the latest release is supported.

## How the app runs Claude Code

OpenPaper has no API key and makes no HTTP calls to Anthropic. It runs the user's own Claude Code CLI as `claude -p` with:

- tools, MCP servers, and user settings turned off
- a scrubbed environment
- an empty temporary folder as the working directory

Paper text goes in and JSON or text comes back. The model cannot read or write files or run commands through the app. Papers and translations stay in `~/Library/Application Support/dev.rlaope.openpaper/papers/`.

Apart from Claude Code's own connection, the app talks only to arXiv (search and import) and GitHub (the update check and installer). The in-app update reruns the official `install.sh` from this repository.
