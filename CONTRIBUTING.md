# Contributing

Thanks for helping. Bug reports, fixes, and new target languages are all welcome. For a larger change, open an issue first so we can agree on the approach.

## Setup

You need macOS, Rust 1.85+, Node 20+, pnpm, and a logged-in Claude Code CLI (`claude auth status`).

```sh
pnpm install
pnpm dev                          # run the app with hot reload
pnpm dev:web                      # frontend only, in a browser
```

Before you open a pull request:

```sh
pnpm build                        # typecheck + web build
cd src-tauri
cargo fmt --check
cargo test --lib
cargo clippy --all-targets -- -D warnings
cargo clippy --features probe --bin openpaper-probe -- -D warnings
```

CI (`.github/workflows/ci.yml`) runs the same checks plus `shellcheck` on every pull request, and `main` only accepts changes that pass it.

[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) maps each stage to its file and describes the `openpaper-probe` CLI, which runs import, translation, search, and chat without the GUI.

## Rules of the codebase

- Model calls go only through the Claude Code CLI, via `translate::run_claude_with`. No API keys, no HTTP calls to Anthropic.
- The AI finder never shows a paper that did not come back from arXiv.
- Keep at least 1 s between arXiv API requests (`ARXIV_GAP`).
- UI copy is short plain English in sentence case, without exclamation marks or emoji.
- Colors are CSS variables in `src/style.css`, defined for both light (`:root`) and dark (`:root[data-theme='dark']`).
- For UI changes, check the running app or a browser with a mocked `window.__TAURI_INTERNALS__`, and include a screenshot in the pull request.

## Commits

One logical change per commit. Messages in English, imperative mood, for example `Add German caption words`. Keep pull requests focused.

## Adding a target language

The language list lives in one place, `src-tauri/src/lang.rs`; the frontend reads it through the `languages` command.

1. Add a variant to `enum Lang` and to `Lang::ALL`.
2. Fill in its `code` (lowercase, used in file names such as `<id>.<code>.json`), English `name`, `native` name, `caption_words` (the words for "Figure" and "Table"), and `register` (the academic writing style to use).
3. Check the per-language `match` arms in `src-tauri/src/ask.rs` (chat and summary length) and `src-tauri/src/recommend.rs` (finder prompts), and add the new language there.
4. Non-Korean languages share the parameterized English prompts in `translate.rs`, `ask.rs`, and `recommend.rs`, so usually nothing else changes. Korean has its own prompts; do not edit them for another language.
5. Update the tests in `lang.rs`, run `cargo test --lib`, and translate a few paragraphs with the probe to check the output reads naturally.

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
