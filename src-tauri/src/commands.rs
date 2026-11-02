//! Tauri commands and the translation job runner.

use crate::lang::{self, Lang, LangInfo};
use crate::library::{Folder, Library, Meta};
use crate::model::{self, Doc};
use crate::translate::Mode;
use crate::{arxiv, ask, pdf, recommend, search, translate, update};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::State;

const WORKERS: usize = 3;

#[derive(Default)]
pub(crate) struct Job {
    /// Units to translate in this job and units finished so far.
    total: usize,
    done: usize,
    running: bool,
    /// Retranslating everything in the current style while the old text stays on screen.
    redo: bool,
    error: Option<String>,
    cancel: Arc<AtomicBool>,
}

pub struct AppState {
    pub library: Library,
    /// Keyed by paper id, mode and language; a translation and an easy explanation, or two
    /// languages, can run at once.
    pub(crate) jobs: Mutex<HashMap<(String, Mode, Lang), Job>>,
}

impl AppState {
    pub fn new(root: PathBuf) -> std::io::Result<Self> {
        Ok(Self {
            library: Library::open(root)?,
            jobs: Mutex::new(HashMap::new()),
        })
    }

    pub fn cancel_all(&self) {
        for job in self.jobs.lock().unwrap().values() {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub total: usize,
    pub done: usize,
    pub running: bool,
    pub error: Option<String>,
    pub redo: bool,
    /// The saved translation was written in an older style and can be redone (Korean only).
    pub outdated: bool,
    /// Unit id → translated sentences.
    pub units: HashMap<u32, Vec<String>>,
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| "The task stopped unexpectedly".to_string())?
}

/// Translation target languages, in display order.
#[tauri::command]
pub fn languages() -> Vec<LangInfo> {
    lang::all()
}

#[tauri::command]
pub fn library_list(state: State<'_, Arc<AppState>>) -> Vec<Meta> {
    state.library.list()
}

#[tauri::command]
pub fn library_get(state: State<'_, Arc<AppState>>, id: String) -> Result<Doc, String> {
    state.library.doc(&id)
}

#[tauri::command]
pub fn library_folders(state: State<'_, Arc<AppState>>) -> Vec<Folder> {
    state.library.folders()
}

#[tauri::command]
pub fn library_set_folders(
    state: State<'_, Arc<AppState>>,
    folders: Vec<Folder>,
) -> Result<Vec<Folder>, String> {
    state.library.set_folders(folders)
}

#[tauri::command]
pub fn library_remove(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    state.jobs.lock().unwrap().retain(|(doc, _, _), job| {
        if *doc == id {
            job.cancel.store(true, Ordering::Relaxed);
        }
        *doc != id
    });
    state.library.remove(&id)
}

#[tauri::command]
pub async fn open_arxiv(state: State<'_, Arc<AppState>>, input: String) -> Result<Meta, String> {
    let id = arxiv::parse_id(&input).ok_or_else(|| {
        "Enter an arXiv ID or arxiv.org link (for example 1706.03762)".to_string()
    })?;
    let state = state.inner().clone();
    blocking(move || {
        let doc = arxiv::fetch(&id)?;
        state.library.save(&doc)
    })
    .await
}

#[tauri::command]
pub async fn arxiv_search(query: String, start: usize) -> Result<search::Page, String> {
    blocking(move || search::search(&query, start)).await
}

#[tauri::command]
pub async fn claude_status() -> Result<translate::ClaudeStatus, String> {
    blocking(|| Ok(translate::claude_status())).await
}

/// AI paper finder: Claude plans arXiv searches for the request, then picks from real results.
/// The reply and reasons are written in `lang`.
#[tauri::command]
pub async fn ai_recommend(
    request: String,
    history: Vec<recommend::Turn>,
    lang: Lang,
) -> Result<recommend::Answer, String> {
    blocking(move || recommend::recommend(&request, &history, lang)).await
}

#[derive(Serialize)]
pub struct AskReply {
    pub reply: String,
}

/// Streamed to the frontend while Claude writes an answer.
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AskEvent {
    Delta { text: String },
}

/// Answers a question about the open paper (`doc_id`), optionally about a selected passage,
/// in `lang`. Pieces of the answer arrive on `on_event` as they are written; the full answer is
/// returned.
#[tauri::command]
pub async fn ask_claude(
    state: State<'_, Arc<AppState>>,
    question: String,
    quote: Option<String>,
    doc_id: Option<String>,
    history: Vec<recommend::Turn>,
    lang: Lang,
    on_event: tauri::ipc::Channel<AskEvent>,
) -> Result<AskReply, String> {
    let state = state.inner().clone();
    blocking(move || {
        ask_paper_stream(
            &state,
            &question,
            quote.as_deref(),
            doc_id.as_deref(),
            &history,
            lang,
            |d| {
                let _ = on_event.send(AskEvent::Delta {
                    text: d.to_string(),
                });
            },
        )
    })
    .await
}

pub fn ask_paper(
    state: &Arc<AppState>,
    question: &str,
    quote: Option<&str>,
    doc_id: Option<&str>,
    history: &[recommend::Turn],
    lang: Lang,
) -> Result<AskReply, String> {
    ask_paper_stream(state, question, quote, doc_id, history, lang, |_| {})
}

pub fn ask_paper_stream(
    state: &Arc<AppState>,
    question: &str,
    quote: Option<&str>,
    doc_id: Option<&str>,
    history: &[recommend::Turn],
    lang: Lang,
    on_delta: impl FnMut(&str),
) -> Result<AskReply, String> {
    let doc = match doc_id {
        Some(id) => Some(
            state
                .library
                .doc(id)
                .map_err(|_| "Couldn't find the paper".to_string())?,
        ),
        None => None,
    };
    let reply = ask::ask_stream(doc.as_ref(), question, quote, history, lang, on_delta)?;
    Ok(AskReply { reply })
}

#[derive(Serialize)]
pub struct SummaryReply {
    pub reply: String,
    /// True when the summary came from disk instead of a new Claude call.
    pub saved: bool,
}

/// An explainer post about a saved paper in `lang`. Returns the saved one unless `fresh`;
/// a new one streams over `on_event` and is saved when done.
#[tauri::command]
pub async fn summarize_paper(
    state: State<'_, Arc<AppState>>,
    doc_id: String,
    fresh: bool,
    lang: Lang,
    on_event: tauri::ipc::Channel<AskEvent>,
) -> Result<SummaryReply, String> {
    let state = state.inner().clone();
    blocking(move || {
        summarize(&state, &doc_id, fresh, lang, |d| {
            let _ = on_event.send(AskEvent::Delta {
                text: d.to_string(),
            });
        })
    })
    .await
}

pub fn summarize(
    state: &Arc<AppState>,
    doc_id: &str,
    fresh: bool,
    lang: Lang,
    on_delta: impl FnMut(&str),
) -> Result<SummaryReply, String> {
    if !fresh {
        if let Some(reply) = state.library.summary(doc_id, lang) {
            return Ok(SummaryReply { reply, saved: true });
        }
    }
    let doc = state
        .library
        .doc(doc_id)
        .map_err(|_| "Couldn't find the paper".to_string())?;
    let reply = ask::summarize_stream(&doc, lang, on_delta)?;
    state.library.set_summary(doc_id, lang, &reply)?;
    Ok(SummaryReply {
        reply,
        saved: false,
    })
}

/// Current version and the newest GitHub release. Network errors mean "no update known".
#[tauri::command]
pub async fn update_check() -> Result<update::UpdateInfo, String> {
    blocking(|| Ok(update::check())).await
}

/// Installs the latest release into the folder this app lives in. On macOS the installer quits,
/// replaces and reopens the app. On Windows the setup program runs once the app has quit, so the
/// app quits itself here. Runs off the main thread: Windows downloads the setup program first.
#[tauri::command]
pub async fn update_install(app: tauri::AppHandle) -> Result<(), String> {
    blocking(|| {
        let exe =
            std::env::current_exe().map_err(|_| "Couldn't find where the app is".to_string())?;
        update::install_latest(&exe, &std::env::temp_dir().join(update::LOG_NAME))
    })
    .await?;
    if cfg!(windows) {
        // Give the reply a moment to reach the window before quitting.
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            app.exit(0);
        });
    }
    Ok(())
}

/// Opens the paper's abstract page on arxiv.org in the default browser. Only arXiv IDs are accepted,
/// so the webview can never open an arbitrary URL.
#[tauri::command]
pub fn open_arxiv_page(app: tauri::AppHandle, id: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let id = arxiv::parse_id(&id).ok_or_else(|| "Not an arXiv ID".to_string())?;
    app.opener()
        .open_url(format!("https://arxiv.org/abs/{id}"), None::<&str>)
        .map_err(|_| "Couldn't open the browser".to_string())
}

#[tauri::command]
pub async fn open_pdf(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<Meta>, String> {
    use tauri_plugin_dialog::DialogExt;
    let state = state.inner().clone();
    blocking(move || {
        let Some(picked) = app
            .dialog()
            .file()
            .add_filter("PDF", &["pdf"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = picked
            .into_path()
            .map_err(|_| "Couldn't read the file path".to_string())?;
        let bytes = std::fs::read(&path).map_err(|_| "Couldn't read the file".to_string())?;
        let mut builder = pdf::parse(&bytes)?;
        if builder.title.is_empty() {
            builder.title = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "PDF".into());
        }
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let doc = builder.finish(format!("pdf-{}", model::content_id(&bytes)), &name);
        state.library.save(&doc).map(Some)
    })
    .await
}

#[tauri::command]
pub fn translation_cancel(state: State<'_, Arc<AppState>>, id: String, mode: Mode, lang: Lang) {
    if let Some(job) = state.jobs.lock().unwrap().get(&(id, mode, lang)) {
        job.cancel.store(true, Ordering::Relaxed);
    }
}

/// Translates every unit that has no saved translation in `lang` yet.
#[tauri::command]
pub fn translation_start(
    state: State<'_, Arc<AppState>>,
    id: String,
    mode: Mode,
    lang: Lang,
) -> Result<(), String> {
    start_translation(state.inner(), id, mode, lang, false)
}

/// Translates the whole paper again in the current style, replacing units as they arrive.
#[tauri::command]
pub fn translation_redo(
    state: State<'_, Arc<AppState>>,
    id: String,
    lang: Lang,
) -> Result<(), String> {
    start_translation(state.inner(), id, Mode::Plain, lang, true)
}

#[tauri::command]
pub fn translation_status(
    state: State<'_, Arc<AppState>>,
    id: String,
    mode: Mode,
    lang: Lang,
) -> Result<Status, String> {
    Ok(status(state.inner(), &id, mode, lang))
}

/// Only the Korean plain translation has older styles; everything else started at the current one.
fn has_styles(mode: Mode, lang: Lang) -> bool {
    mode == Mode::Plain && lang == Lang::Ko
}

pub fn status(state: &Arc<AppState>, id: &str, mode: Mode, lang: Lang) -> Status {
    let units = state.library.translations(id, mode, lang);
    let jobs = state.jobs.lock().unwrap();
    let job = jobs.get(&(id.to_string(), mode, lang));
    Status {
        total: job.map_or(0, |j| j.total),
        done: job.map_or(0, |j| j.done),
        running: job.is_some_and(|j| j.running),
        error: job.and_then(|j| j.error.clone()),
        redo: job.is_some_and(|j| j.running && j.redo),
        outdated: has_styles(mode, lang)
            && !units.is_empty()
            && state.library.style(id) < translate::STYLE,
        units,
    }
}

pub fn start_translation(
    state: &Arc<AppState>,
    id: String,
    mode: Mode,
    lang: Lang,
    redo: bool,
) -> Result<(), String> {
    let claude = translate::find_claude().ok_or_else(|| translate::NOT_INSTALLED.to_string())?;
    let doc = state.library.doc(&id)?;
    let have = state.library.translations(&id, mode, lang);
    let pending: Vec<model::Unit> = doc
        .units()
        .into_iter()
        .filter(|u| redo || !have.contains_key(&u.id))
        .cloned()
        .collect();
    let batches = translate::batches(&pending);
    let headings = doc.heading_ids();
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut jobs = state.jobs.lock().unwrap();
        let key = (id.clone(), mode, lang);
        if jobs.get(&key).is_some_and(|j| j.running) {
            return Ok(());
        }
        jobs.insert(
            key,
            Job {
                total: pending.len(),
                done: 0,
                running: !batches.is_empty(),
                redo,
                error: None,
                cancel: cancel.clone(),
            },
        );
    }
    if batches.is_empty() {
        return Ok(());
    }
    // A paper translated from scratch is entirely in the current style. A redo only counts once
    // every unit has been replaced; until then the old style stays marked as outdated.
    if has_styles(mode, lang) && have.is_empty() {
        state.library.set_style(&id, translate::STYLE)?;
    }
    let state = state.clone();
    std::thread::spawn(move || {
        let queue = Mutex::new(batches.into_iter());
        let first_error: Mutex<Option<String>> = Mutex::new(None);
        std::thread::scope(|scope| {
            for _ in 0..WORKERS {
                scope.spawn(|| {
                    loop {
                        if cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        let Some(batch) = queue.lock().unwrap().next() else {
                            return;
                        };
                        match translate::translate_batch(
                            &claude, &doc.title, &batch, &headings, mode, lang, &cancel,
                        ) {
                            Ok(units) => {
                                let _ = state.library.add_translations(&id, mode, lang, &units);
                                if let Some(job) =
                                    state
                                        .jobs
                                        .lock()
                                        .unwrap()
                                        .get_mut(&(id.clone(), mode, lang))
                                {
                                    job.done += batch.len();
                                }
                            }
                            Err(e) => {
                                first_error.lock().unwrap().get_or_insert(e);
                                cancel.store(true, Ordering::Relaxed);
                                return;
                            }
                        }
                    }
                });
            }
        });
        let error = first_error.into_inner().unwrap();
        if has_styles(mode, lang) && redo && error.is_none() && !cancel.load(Ordering::Relaxed) {
            let _ = state.library.set_style(&id, translate::STYLE);
        }
        if let Some(job) = state
            .jobs
            .lock()
            .unwrap()
            .get_mut(&(id.clone(), mode, lang))
        {
            job.running = false;
            job.error = error;
        }
    });
    Ok(())
}
