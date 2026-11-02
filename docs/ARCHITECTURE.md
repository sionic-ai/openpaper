# Architecture

OpenPaper is a Tauri 2 app for macOS and Windows: a Rust core in `src-tauri/` and a React 19 / Vite frontend in `src/`. Every model call goes through the user's logged-in Claude Code CLI.

## Pipeline

| Stage | File | What it does |
|---|---|---|
| arXiv import | `src-tauri/src/arxiv.rs` | Parses the arXiv HTML (LaTeXML) into sections, paragraphs, figures, tables, and equations. Falls back to the PDF when there is no HTML version. Figures are embedded as data URLs. |
| Search | `src-tauri/src/search.rs` | Queries the public arXiv API (`export.arxiv.org/api/query`) for papers containing every word, 20 per page by relevance, and parses the Atom response. Quoted phrases are matched as phrases. Opening arxiv.org only accepts an ID and always opens `https://arxiv.org/abs/<id>`. |
| AI finder | `src-tauri/src/recommend.rs` | Claude turns the request into up to 3 English queries plus arXiv IDs it is sure about. The app fetches real results from the arXiv API, then Claude picks at most 5 from those results. Any ID not in the candidates is dropped, so a link to a paper that does not exist can never appear. One request costs 2 Claude calls and up to 4 arXiv requests, about 15 seconds. |
| Ask and summary | `src-tauri/src/ask.rs` | Runs `claude -p --output-format stream-json` and streams pieces to the UI over a Tauri Channel. The open paper is rebuilt as English plain text with equations restored (up to 60,000 characters; if the selection is late in the paper, the window moves around it) and sent with the selected passage and the last 8 chat turns. The summary post uses the same path with its own prompt. Answers are written in the selected target language. |
| PDF import | `src-tauri/src/pdf.rs` | Extracts text, then rebuilds the title and numbered sections. Drops page numbers and rejoins words hyphenated at line ends by checking the document's own vocabulary. |
| Sentences | `src-tauri/src/sentence.rs` | Splits sentences while respecting academic abbreviations (e.g., Fig., et al.). |
| Translation | `src-tauri/src/translate.rs` | Runs `claude -p` with tools, MCP, and settings off in an empty temp folder. Expects JSON whose paragraph IDs and sentence counts match the source 1:1, and retries any paragraph that does not match on its own. Runs 3 batches at a time. Jobs are keyed by paper, mode, and target language. |
| Languages | `src-tauri/src/lang.rs` | The list of target languages. The frontend reads it through the `languages` command. Korean keeps its own detailed prompts; other languages use parameterized English prompts. |
| Claude lookup | `src-tauri/src/translate.rs` | Looks for the CLI at `~/.local/bin/claude`, `~/.claude/local/claude`, Homebrew paths, then `PATH`. On Windows: `%USERPROFILE%\.local\bin\claude.exe`, then `claude.exe` on `PATH`; child processes get no console window. Checks login with `claude auth status`, which makes no model call. |
| Update | `src-tauri/src/update.rs` | Reads the latest tag from the redirect of `releases/latest` (GitHub API as fallback; the button stays hidden if both fail). macOS: installing runs that tag's `install.sh` detached from the app with `OPENPAPER_DIR=<folder of the running app>` and `OPENPAPER_NO_CLAUDE=1`. Windows: an update is offered only when the release has `OpenPaper-windows-x64-setup.exe`; installing downloads it, checks its `.sha256`, starts a hidden PowerShell that waits for the app to quit, runs the setup with `/S` and reopens the app, and the app quits itself. Both log to `$TMPDIR/openpaper-update.log` (`%TEMP%` on Windows). |
| Reader | `src/Reader.tsx` | Lays out each row as a 2-column grid of original and translation. Inline math is kept through translation as `⟦M n⟧` tokens and rendered with KaTeX. Hover changes a single CSS rule so both sentences with the same `data-s` light up. The reading position (the original sentence at the top of the view) is saved per paper in `localStorage` (`src/position.ts`). |
| Find | `src/Find.tsx` | `⌘F` searches the text of both columns and paints matches with the CSS Highlight API, so the paper's DOM is not changed. |

## Library files

Everything lives in `~/Library/Application Support/dev.rlaope.openpaper/papers/` (Windows: `%APPDATA%\dev.rlaope.openpaper\papers\`) as plain JSON. There is no database.

| What | Korean (`ko`) | Other languages |
|---|---|---|
| Translation | `<id>.ko.json` | `<id>.<lang>.json` |
| Easy explanation | `<id>.easy.json` | `<id>.<lang>.easy.json` |
| Summary post | `<id>.summary.json` | `<id>.<lang>.summary.json` |
| Style version | `<id>.style.json` (Korean plain translation only) | none |

Folders are stored in `folders.json`. Removing a paper deletes all of its files for every language.

On first launch, if the new `papers` folder does not exist and `~/Library/Application Support/dev.rlaope.nanran/papers` (from the app's former name, Nanran) does, the app moves it over. A failed move does not stop startup.

## Command-line probe

A dev-only binary checks each stage without the GUI. It is gated behind the `probe` feature so `tauri build` does not bundle it.

```sh
cd src-tauri
P="cargo run --release --features probe --bin openpaper-probe --"
$P claude -                               # Claude Code install and login state
$P search 'flash attention'               # arXiv search
$P ask 'papers on KV cache compression'   # AI paper finder
$P update                                 # compare with the latest GitHub release
$P askpaper arxiv-1706.03762 'What is the key idea?'   # ask about a paper in the app's library
$P summary arxiv-1706.03762 /tmp/lib-copy # write the summary post into a library copy
$P parse 1706.03762                       # block statistics
$P dump paper.pdf                         # one line per block
$P translate 1706.03762 6                 # translate the first 6 units
$P job paper.pdf /tmp/lib                 # the full translation job, as in the app
$P redo <doc-id> /tmp/lib                 # retranslate a saved paper in the current style
$P selfupdate <path to OpenPaper.app/Contents/MacOS/openpaper>   # run the in-app updater for that copy
OPENPAPER_MODE=easy $P translate 2510.15103 6   # easy mode instead
OPENPAPER_LANG=ja $P translate 1706.03762 6     # another target language
```

`OPENPAPER_LANG=<code>` picks the target language (`ko ja zh es fr de pt`, default `ko`) for `translate`, `job`, `import`, `redo`, `summary`, `askpaper`, and `ask`. An unknown code, or a command with missing arguments, prints usage and exits 2.
