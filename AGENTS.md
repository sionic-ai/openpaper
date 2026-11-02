# AGENTS.md

Instructions for coding agents (Claude Code, Codex, ...) that are asked to **install** OpenPaper for a user or to **work on** this repository. OpenPaper is a Tauri 2 app for macOS and Windows that shows an English research paper side by side with its translation in a language the user picks. Translation, summaries, chat, and the AI paper finder run through the user's logged-in Claude Code CLI; there is no API key.

## Installing for a user

Run the one-line installer. It is idempotent: running it again updates in place.

```sh
curl -fsSL https://raw.githubusercontent.com/sionic-ai/openpaper/main/install.sh | bash -s -- --no-login
```

Use `--no-login` when you run without an interactive terminal. Login is browser-based OAuth and must be done by the human. Never type, paste, or ask for the user's credentials.

Then read the result from the exit code:

| Exit | Meaning | What to do |
|---|---|---|
| `0` | App installed, Claude Code installed and logged in | Done. Tell the user the app is in `/Applications/OpenPaper.app` (or `~/Applications`) |
| `1` | Error (message on stderr) | See troubleshooting below |
| `2` | App installed, Claude Code is not logged in | Ask the user to run `claude auth login` in their own terminal. Then verify with `--check` |

Verify at any time without changing anything:

```sh
curl -fsSL https://raw.githubusercontent.com/sionic-ai/openpaper/main/install.sh | bash -s -- --check
```

```
app: /Applications/OpenPaper.app
app_version: 0.2.0
claude: /Users/<you>/.local/bin/claude
claude_login: yes
```

`--check` exits `0` when all of these are ready, `1` when the app is missing, and `2` when the app is there but Claude Code is missing or not logged in. `claude auth status` prints JSON with `"loggedIn": true|false` if you need to check Claude alone.

Other flags: `--no-open` (do not launch the app), `--no-claude` (skip Claude Code entirely), `--dir <path>`, `--version <tag>`, `--uninstall` (remove the app, keep papers), `--purge` (with `--uninstall`, also delete papers). The same options exist as env vars: `OPENPAPER_NO_OPEN=1`, `OPENPAPER_NO_CLAUDE=1`, `OPENPAPER_NO_LOGIN=1`, `OPENPAPER_DIR`, `OPENPAPER_VERSION`.

What the installer touches, so you can report it:

- Downloads `OpenPaper-macos-universal.zip` from the latest GitHub release of `sionic-ai/openpaper` and checks it against the release's `.sha256` file before installing
- Writes `OpenPaper.app` to `/Applications` (falls back to `~/Applications` if not writable). Quits a running OpenPaper or Nanran in that folder first
- Removes a leftover `Nanran.app` (the app's former name) from the same folder after a successful install. The app moves the old library from `~/Library/Application Support/dev.rlaope.nanran/papers` on first launch
- Removes the `com.apple.quarantine` xattr from the app. The build is ad-hoc signed and not notarized, so Gatekeeper would otherwise block it
- If `claude` is missing, runs Anthropic's official `https://claude.ai/install.sh`, which installs to `~/.local/bin/claude`
- Does not use `sudo`, does not edit shell profiles, and stores no secrets

Troubleshooting:

- `OpenPaper runs on macOS only.` → Linux and Windows are not supported
- `Couldn't download ...` → network problem, or the `--version` tag does not exist. List tags with `gh release list -R sionic-ai/openpaper`
- The app says Claude Code is not logged in → have the user run `claude auth login`. The app checks again when its window regains focus
- Translation fails with a usage-limit message → the Claude account hit its limit. Nothing to fix locally
- User data lives in `~/Library/Application Support/dev.rlaope.openpaper/papers/`. Do not delete it unless asked

Windows has no one-line installer. Download `OpenPaper-windows-x64-setup.exe` from the latest release and run it (per-user, no admin; `/S` installs silently). The app data is in `%APPDATA%\dev.rlaope.openpaper\papers\`. Claude Code on Windows must be the native `claude.exe` (`irm https://claude.ai/install.ps1 | iex`).

Uninstall: run the installer with `--uninstall`, or `rm -rf /Applications/OpenPaper.app`. Delete the data folder above (or pass `--purge`) only if the user asks.

## Working on the code

Stack: Tauri 2 + Rust (`src-tauri/`) + React 19 / Vite (`src/`). macOS and Windows; keep platform code behind `#[cfg(windows)]` / `#[cfg(not(windows))]` in Rust and `src/platform.ts` in the frontend (⌘ on macOS, Ctrl on Windows). CI compiles, lints and tests on both. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) maps each pipeline stage to its file and lists the library file formats.

```sh
pnpm install
pnpm dev                                    # run the app
pnpm build                                  # typecheck + web build
cd src-tauri && cargo test --lib && cargo clippy --all-targets
cargo run --release --features probe --bin openpaper-probe -- claude -   # CLI probes, see docs/ARCHITECTURE.md
```

Rules:

- **Claude only through the CLI.** No API keys and no HTTP calls to Anthropic. `translate::run_claude_with` runs `claude -p` with a scrubbed env, no tools/MCP/settings, in an empty temp dir. Reuse it for new prompts.
- **Never show model-invented papers.** The AI finder (`recommend.rs`) lets Claude plan queries, fetches real arXiv results, and keeps only picks whose IDs are in those results. Keep that invariant.
- **Be polite to arXiv.** Keep at least 1 s between API requests (`ARXIV_GAP`).
- **UI copy and error messages are short plain English in sentence case**, e.g. `Couldn't start the translation`. No exclamation marks, no emoji. Translation, summary, and chat output use the selected target language (`src-tauri/src/lang.rs` is the single list). The Korean prompts are product data; leave them in Korean.
- Keep the minimal design. Colors live in CSS variables in `src/style.css` (`--accent` is the accent); light is `:root`, dark is `:root[data-theme='dark']`, so new styles must use variables, not raw hex.
- The library is plain JSON files under the app data dir. There is no database. On first launch the app moves an existing library from the old `dev.rlaope.nanran` data dir into `dev.rlaope.openpaper`.
- Verify UI changes in the running app or with a mocked `window.__TAURI_INTERNALS__` in a browser (`pnpm dev:web`). Run `pnpm build` and `cargo test --lib` before committing.
- Make small commits, one logical change each. Write commit messages in English, imperative mood.

## Releasing (maintainers)

Bump the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`, merge to `main`, then push a matching tag. The `Release` workflow (`.github/workflows/release.yml`) runs `scripts/release.sh` on a macOS runner:

```sh
git tag v0.2.1 && git push origin v0.2.1
scripts/release.sh v0.2.1     # the same thing by hand; bumps nothing
```

The `windows` job then builds the NSIS installer on a Windows runner and adds `OpenPaper-windows-x64-setup.exe` plus its `.sha256` to the same release; the Windows app offers an update only once that asset is there. Running the workflow by hand (`workflow_dispatch`) builds the Windows installer as an artifact without publishing. The script builds `--target universal-apple-darwin`, zips `OpenPaper.app` with `ditto` as `OpenPaper-macos-universal.zip` plus its `.sha256`, and creates or updates the GitHub release for that tag on `sionic-ai/openpaper`. `install.sh` always downloads that exact asset name, so keep the name unchanged. The in-app Update button runs the `install.sh` of the release tag it found, not the one on `main`, so installer changes reach users only through a release. `scripts/check-versions.sh [tag]` checks that the four version fields (including `Cargo.lock`) agree; CI runs it on every push and the release runs it with the tag first. The tag must equal `v` + the app version, and replacing assets of an existing release needs `OPENPAPER_CLOBBER=1`.
