<div align="center">

<img src="src-tauri/icons/128x128@2x.png" alt="OpenPaper icon" width="128" height="128">

# OpenPaper

Read an English research paper side by side with its translation, sentence by sentence.

[![Release](https://img.shields.io/github/v/release/sionic-ai/openpaper?style=flat-square&labelColor=black&color=white)](https://github.com/sionic-ai/openpaper/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-white?style=flat-square&labelColor=black)](LICENSE)
[![Platform: macOS](https://img.shields.io/badge/platform-macOS-white?style=flat-square&labelColor=black&logo=apple)](#install)
[![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%202-white?style=flat-square&labelColor=black&logo=tauri)](https://tauri.app)

</div>

<a href=".github/assets/demo.mp4"><img src=".github/assets/demo.webp" alt="OpenPaper demo: search, translate, summarize, ask, and linked sentence highlight" width="100%"></a>

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/sionic-ai/openpaper/main/install.sh | bash
```

Run it again to update. The app also shows an Update button when a new release is out.

The installer puts `OpenPaper.app` in `/Applications` (or `~/Applications`), installs [Claude Code](https://docs.claude.com/claude-code) if it is missing, and starts the Claude Code login if you are not logged in. Translation uses your Claude Code account, so there is no API key to set. Requires macOS 11 or later and a Claude account.

<details>
<summary>Installer options</summary>

Options go after `bash -s --`, for example `... | bash -s -- --no-open`:

| Option | Env var | What it does |
|---|---|---|
| `--no-claude` | `OPENPAPER_NO_CLAUDE=1` | Skip installing and logging in to Claude Code |
| `--no-login` | `OPENPAPER_NO_LOGIN=1` | Never start the Claude Code login |
| `--no-open` | `OPENPAPER_NO_OPEN=1` | Do not open the app at the end |
| `--dir <path>` | `OPENPAPER_DIR` | Install into this folder |
| `--version <tag>` | `OPENPAPER_VERSION` | Install a specific release tag |
| `--check` | | Print what is installed and change nothing |
| `--uninstall` | | Remove the app, keep your papers |
| `--purge` | | With `--uninstall`, also delete your papers |

Exit codes: `0` ready, `1` error, `2` app installed but Claude Code is not logged in. Agents installing for a user should follow [AGENTS.md](AGENTS.md).

</details>

## What you get

- **Side by side reading.** The original on one side, the translation on the other, paired sentence by sentence. Hover or select a sentence and its pair lights up on both sides. Swap sides with `⇄` and drag the divider to resize.
- **Pick your language.** Translate into Korean, Japanese, Chinese (Simplified), Spanish, French, German, or Portuguese.
- **arXiv search.** Search arXiv inside the app, or paste an arXiv ID or URL. Local PDFs work too.
- **AI paper finder.** Describe what you are looking for in plain words. Claude plans the searches, and the app only shows papers that came back from arXiv, never invented ones.
- **Easy mode.** An explanatory translation that unpacks jargon and adds the missing reasoning, while keeping facts and numbers as written.
- **Summary post.** One click writes a short explainer post about the paper with its real numbers. It is saved and shows up instantly next time.
- **Ask the paper.** Ask questions in a chat grounded in the open paper. Select any passage to ask about it directly. Answers stream in as they are written.
- **Folders and dark mode.** Drag papers onto each other to group them. Light and dark themes follow macOS until you pick one.
- **In-app update.** An Update button appears when a new release is out.

## How it works

- **Claude Code CLI, isolated.** Every model call runs `claude -p` with tools, MCP servers, and settings turned off, in an empty temporary folder. The app sends paper text and reads JSON or text back. No API key, no direct HTTP calls to Anthropic.
- **No invented papers.** The AI finder lets Claude plan queries, fetches real results from the arXiv API, and keeps only picks whose IDs are in those results.
- **Your files stay local.** Papers, translations, summaries, and folders are plain JSON files in `~/Library/Application Support/dev.rlaope.openpaper/papers/`.

The full pipeline, file formats, and the command-line probe are in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Build from source

Needs Rust 1.85+, Node 20+, pnpm, and a logged-in `claude` CLI (`claude auth status`).

```sh
pnpm install
pnpm dev          # run the app in development
pnpm build        # typecheck and build the web side
pnpm tauri build  # src-tauri/target/release/bundle/macos/OpenPaper.app
```

## Uninstall

```sh
curl -fsSL https://raw.githubusercontent.com/sionic-ai/openpaper/main/install.sh | bash -s -- --uninstall
```

Add `--purge` to also delete your papers. By hand: `rm -rf /Applications/OpenPaper.app`, and `~/Library/Application Support/dev.rlaope.openpaper` for the papers.

## Limits

- macOS only.
- The build is ad-hoc signed, not notarized. The installer clears the download quarantine. If you download the zip in a browser, right-click the app and choose Open the first time.
- PDFs use the text layer only. Scanned PDFs do not work, and PDF equations, figures, and tables show as plain text.
- Speed and quality follow your Claude Code account's model and usage limits.

## Contributing

Issues and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md). Report security issues privately as described in [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE)
