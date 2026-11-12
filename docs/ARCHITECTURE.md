# Architecture

OpenPaper is a Tauri 2 app for macOS and Windows: a Rust core in `src-tauri/` and a React 19 / Vite frontend in `src/`. Every model call goes through the user's logged-in Claude Code CLI.

## Pipeline

| Stage | File | What it does |
|---|---|---|
| Other sites | `src-tauri/src/link.rs` | A pasted paper link or DOI opens through the same `open_arxiv` command. The arXiv version wins when one is found (an arXiv ID in the link, a redirect to arXiv, `citation_arxiv_id`, an arXiv location in OpenAlex, or an arXiv paper with exactly the same title); otherwise the page's `citation_pdf_url`, the response itself when it is a PDF, or an open-access PDF from OpenAlex is read like a PDF file. Links to local or private addresses are refused, redirects included. Sites without a readable PDF (paywalls such as ACM or IEEE) end with a message to download the PDF. |
| arXiv import | `src-tauri/src/arxiv.rs` | Parses the arXiv HTML (LaTeXML) into sections, paragraphs, figures, tables, and equations. Falls back to the PDF when there is no HTML version. Figures are embedded as data URLs. |
| Search | `src-tauri/src/search.rs` | Queries the public arXiv API (`export.arxiv.org/api/query`) for papers containing every word, 20 per page by relevance, and parses the Atom response. Quoted phrases are matched as phrases. Opening arxiv.org only accepts an ID and always opens `https://arxiv.org/abs/<id>`. |
| AI finder | `src-tauri/src/recommend.rs` | Claude turns the request into up to 3 English queries plus arXiv IDs it is sure about. The app fetches real results from the arXiv API, then Claude picks at most 5 from those results. Any ID not in the candidates is dropped, so a link to a paper that does not exist can never appear. One request costs 2 Claude calls and up to 4 arXiv requests, about 15 seconds. |
| Ask and summary | `src-tauri/src/ask.rs` | Runs `claude -p --output-format stream-json` and streams pieces to the UI over a Tauri Channel. The open paper is rebuilt as English plain text with equations restored (up to 60,000 characters; if the selection is late in the paper, the window moves around it) and sent with the selected passage and the last 8 chat turns. The summary post uses the same path with its own prompt. Answers are written in the selected target language. |
| PDF import | `src-tauri/src/layout.rs`, `src-tauri/src/pdf.rs` | `layout.rs` reads where every glyph sits and rebuilds lines in reading order: drops the line numbers review templates print in the margins and page numbers, reads two-column pages column by column, keeps figure labels out of the prose, takes the largest text on page one as the title, writes sub- and superscripts as TeX for KaTeX (`R_{0}`, `\mathrm{TopK}_{K_{0},p_{d}}`), and turns aligned rows into tables, lines ending in `(n)` into display equations (fractions included) and numbered pseudocode into code blocks. `pdf.rs` then finds headings, front matter and references and rejoins words hyphenated at line ends by checking the document's own vocabulary. If the layout pass finds no text, it falls back to plain text extraction. |
| Sentences | `src-tauri/src/sentence.rs` | Splits sentences while respecting academic abbreviations (e.g., Fig., et al.). |
| Translation | `src-tauri/src/translate.rs` | Runs `claude -p` with tools, MCP, and settings off in an empty temp folder. Expects JSON whose paragraph IDs and sentence counts match the source 1:1, and retries any paragraph that does not match on its own. A batch reply that can't be read at all (broken JSON, cut off at the length limit, timed out) is retried paragraph by paragraph instead of stopping the job; login, usage-limit and cancel errors still stop it. Runs 3 batches at a time. Jobs are keyed by paper, mode, and target language. |
| Languages | `src-tauri/src/lang.rs` | The list of target languages. The frontend reads it through the `languages` command. Korean keeps its own detailed prompts; other languages use parameterized English prompts. |
| Model backends | `src-tauri/src/llm.rs` | `Backend` runs every model call: Claude Code CLI (default), Codex CLI (`codex exec --json --ephemeral --ignore-user-config --ignore-rules --sandbox read-only` in an empty temp dir; the system prompt goes first in the prompt), Anthropic Messages API, or OpenAI-compatible Chat Completions, both streamed over SSE with no redirects and two retries on 429/5xx. `settings.json` holds the choice and a model per slot (`claude-code`, `codex`, `anthropic`, `openai:<preset>`); `keys.json` holds API keys by slot. |
| Claude lookup | `src-tauri/src/translate.rs` | Looks for the CLI at `~/.local/bin/claude`, `~/.claude/local/claude`, Homebrew paths, then `PATH`. On Windows: `%USERPROFILE%\.local\bin\claude.exe`, then `claude.exe` on `PATH`; child processes get no console window. Checks login with `claude auth status`, which makes no model call. |
| Update | `src-tauri/src/update.rs` | Reads the latest tag from the redirect of `releases/latest` (GitHub API as fallback; the button stays hidden if both fail). macOS: installing runs that tag's `install.sh` detached from the app with `OPENPAPER_DIR=<folder of the running app>` and `OPENPAPER_NO_CLAUDE=1`. Windows: an update is offered only when the release has `OpenPaper-windows-x64-setup.exe`; installing downloads it, checks its `.sha256`, saves it in a new temp folder and starts it with `/S /UPDATE /R`. The NSIS setup closes the app, installs over it and reopens it, and the app quits itself too. Both log to `$TMPDIR/openpaper-update.log` (`%TEMP%` on Windows). |
| Reader | `src/Reader.tsx` | Lays out each row as a 2-column grid of original and translation. Inline math is kept through translation as `⟦M n⟧` tokens and rendered with KaTeX. Hover changes a single CSS rule so both sentences with the same `data-s` light up. The reading position (the original sentence at the top of the view) is saved per paper in `localStorage` (`src/position.ts`). |
| Tabs | `src/App.tsx`, `src-tauri/src/main.rs` | Each tab keeps its own view, paper, arXiv search and roadmap; one paper is loaded at a time and switching tabs reloads it. On macOS the app menu owns `⌘T` (New Tab) and `⌘W` (Close Tab, which closes the window on the last tab) and sends them to the webview as a `menu` event; on Windows the webview handles `Ctrl+T` and `Ctrl+W`. |
| Text size | `src/App.tsx`, `src/style.css` | `⌘+` / `⌘−` / `⌘0` (Ctrl on Windows) set `--zoom` on `.reader` in steps of 10 % between 70 and 150 %, saved in `openpaper.textSize`. Only the paper's text scales; the sidebar, bar and panels keep their size. The keys are matched by `e.code`, so they work with any input source. |
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
