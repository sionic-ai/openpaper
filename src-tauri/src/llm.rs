//! Model backends. Every model call (translation, ask, summary, AI finder) goes through a
//! [`Backend`]: the Claude Code CLI (the default), the Codex CLI, the Anthropic API, or any
//! OpenAI-compatible API (OpenGateway, OpenAI, OpenRouter, DeepSeek, Gemini, a local Ollama, or a
//! custom URL).
//!
//! The choice lives in `settings.json` in the app data folder. API keys live apart from it in
//! `keys.json` (owner-only on macOS) and are never sent back to the webview.

use crate::translate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    #[default]
    ClaudeCode,
    Codex,
    Anthropic,
    Openai,
}

/// OpenAI-compatible endpoints the settings offer by name. `custom` takes a base URL.
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub base: &'static str,
    pub needs_key: bool,
    /// Model used until one is picked, or empty when the user must pick one.
    pub model: &'static str,
    /// Where to create an API key, or empty.
    pub keys: &'static str,
}

pub const PRESETS: &[Preset] = &[
    Preset {
        id: "openai",
        name: "OpenAI",
        base: "https://api.openai.com/v1",
        needs_key: true,
        model: "",
        keys: "https://platform.openai.com/api-keys",
    },
    Preset {
        id: "openrouter",
        name: "OpenRouter",
        base: "https://openrouter.ai/api/v1",
        needs_key: true,
        model: "",
        keys: "https://openrouter.ai/settings/keys",
    },
    Preset {
        id: "deepseek",
        name: "DeepSeek",
        base: "https://api.deepseek.com/v1",
        needs_key: true,
        model: "",
        keys: "https://platform.deepseek.com/api_keys",
    },
    Preset {
        id: "gemini",
        name: "Google Gemini",
        base: "https://generativelanguage.googleapis.com/v1beta/openai",
        needs_key: true,
        model: "",
        keys: "https://aistudio.google.com/apikey",
    },
    Preset {
        id: "opengateway",
        name: "OpenGateway",
        base: "https://apis.opengateway.ai/v1",
        needs_key: true,
        model: "anthropic/claude-sonnet-5-5",
        keys: "https://opengateway.ai/api-keys",
    },
    Preset {
        id: "ollama",
        name: "Ollama (local)",
        base: "http://localhost:11434/v1",
        needs_key: false,
        model: "",
        keys: "",
    },
    Preset {
        id: "custom",
        name: "Custom",
        base: "",
        needs_key: false,
        model: "",
        keys: "",
    },
];

const ANTHROPIC_BASE: &str = "https://api.anthropic.com";
const ANTHROPIC_MODEL: &str = "claude-sonnet-5-5";
const ANTHROPIC_KEYS: &str = "https://console.anthropic.com/settings/keys";
/// Output budget for one Anthropic reply; a translation batch needs a few thousand tokens.
const ANTHROPIC_MAX_TOKENS: u32 = 16_000;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub provider: Provider,
    /// OpenAI-compatible preset id, see [`PRESETS`].
    #[serde(default = "default_preset")]
    pub preset: String,
    /// Base URL for the `custom` preset.
    #[serde(default)]
    pub base_url: String,
    /// Model per slot (see [`Settings::slot`]); a missing entry means the slot's default.
    #[serde(default)]
    pub models: HashMap<String, String>,
    /// Backup slots, in order. A call goes to the next one when the chosen backend hits a usage
    /// limit or can't run.
    #[serde(default)]
    pub backups: Vec<String>,
}

fn default_preset() -> String {
    "openai".into()
}

impl Settings {
    /// Key and model slot of the current choice: `claude-code`, `codex`, `anthropic`, or
    /// `openai:<preset>`.
    pub fn slot(&self) -> String {
        match self.provider {
            Provider::ClaudeCode => "claude-code".into(),
            Provider::Codex => "codex".into(),
            Provider::Anthropic => "anthropic".into(),
            Provider::Openai => format!("openai:{}", self.preset),
        }
    }

    /// These settings with `slot` chosen instead, or `None` for a slot that doesn't exist.
    pub fn with_slot(&self, slot: &str) -> Option<Settings> {
        let (provider, preset) = match slot {
            "claude-code" => (Provider::ClaudeCode, None),
            "codex" => (Provider::Codex, None),
            "anthropic" => (Provider::Anthropic, None),
            _ => {
                let preset = slot.strip_prefix("openai:")?;
                PRESETS.iter().find(|p| p.id == preset)?;
                (Provider::Openai, Some(preset))
            }
        };
        let mut s = self.clone();
        s.provider = provider;
        if let Some(p) = preset {
            s.preset = p.to_string();
        }
        Some(s)
    }

    pub fn model(&self) -> String {
        let set = self
            .models
            .get(&self.slot())
            .map(|m| m.trim())
            .unwrap_or("");
        if !set.is_empty() {
            return set.to_string();
        }
        match self.provider {
            Provider::ClaudeCode => translate::MODEL.into(),
            Provider::Anthropic => ANTHROPIC_MODEL.into(),
            // Codex uses the account's default model; most OpenAI-compatible APIs need a pick.
            Provider::Codex => String::new(),
            Provider::Openai => self.preset().model.into(),
        }
    }

    fn preset(&self) -> &'static Preset {
        PRESETS
            .iter()
            .find(|p| p.id == self.preset)
            .unwrap_or(&PRESETS[0])
    }

    /// Base URL of the OpenAI-compatible endpoint, without a trailing slash.
    pub fn openai_base(&self) -> String {
        let p = self.preset();
        let base = if p.id == "custom" {
            self.base_url.trim()
        } else {
            p.base
        };
        base.trim_end_matches('/').to_string()
    }

    /// Web page where the user creates a key for the current choice, if it takes one.
    pub fn key_page(&self) -> Option<&'static str> {
        let page = match self.provider {
            Provider::ClaudeCode | Provider::Codex => "",
            Provider::Anthropic => ANTHROPIC_KEYS,
            Provider::Openai => self.preset().keys,
        };
        (!page.is_empty()).then_some(page)
    }

    pub fn needs_key(&self) -> bool {
        match self.provider {
            Provider::ClaudeCode | Provider::Codex => false,
            Provider::Anthropic => true,
            Provider::Openai => self.preset().needs_key,
        }
    }
}

/// An address for API calls: https anywhere, or plain http to this computer. Plain http to
/// another machine (a LAN Ollama) is allowed only when no key would travel with it.
pub fn base_allowed(base: &str, with_key: bool) -> bool {
    let Ok(url) = reqwest::Url::parse(base) else {
        return false;
    };
    match url.scheme() {
        "https" => url.host_str().is_some(),
        "http" => {
            matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
                || (!with_key && url.host_str().is_some())
        }
        _ => false,
    }
}

/// Model names go to the CLIs as an argument, so keep them to plain characters.
fn model_ok(model: &str) -> bool {
    !model.starts_with('-')
        && model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '/' | '-' | '@'))
}

const BAD_MODEL: &str = "Model names can only use letters, numbers and . _ : / - @";

/// Settings and keys on disk.
pub struct Store {
    settings_path: PathBuf,
    keys_path: PathBuf,
    /// Which backends are resting after a limit, and which one answered last. Memory only.
    switch: Arc<Mutex<Switch>>,
}

/// How long a backend that hit a usage limit or couldn't run is skipped before it is tried again.
const REST: Duration = Duration::from_secs(20 * 60);

#[derive(Default, Debug)]
pub struct Switch {
    /// Slot → (try again after, why it stopped).
    resting: HashMap<String, (Instant, String)>,
    /// The backup that answered the last call and why the chosen backend didn't, or `None` when
    /// the chosen backend answered.
    serving: Option<(String, String)>,
}

impl Switch {
    fn resting(&self, slot: &str) -> Option<String> {
        self.resting
            .get(slot)
            .filter(|(until, _)| Instant::now() < *until)
            .map(|(_, why)| why.clone())
    }
}

impl Store {
    pub fn new(app_data: &Path) -> Self {
        Self {
            settings_path: app_data.join("settings.json"),
            keys_path: app_data.join("keys.json"),
            switch: Arc::default(),
        }
    }

    /// Forgets resting backends and the last backup, so new settings start with the chosen one.
    pub fn reset_switch(&self) {
        *self.switch.lock().unwrap() = Switch::default();
    }

    /// The backup that answered the last call, while it is one of the saved backups.
    pub fn serving(&self, s: &Settings) -> Option<Backup> {
        let (slot, reason) = self.switch.lock().unwrap().serving.clone()?;
        if slot == s.slot() || !s.backups.contains(&slot) {
            return None;
        }
        let b = s.with_slot(&slot)?;
        Some(Backup {
            provider: b.provider,
            label: label(&b),
            model: b.model(),
            preset: b.preset.clone(),
            reason,
        })
    }

    pub fn settings(&self) -> Settings {
        std::fs::read(&self.settings_path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, s: &Settings) -> Result<(), String> {
        write_private(&self.settings_path, &serde_json::to_vec_pretty(s).unwrap())
            .map_err(|_| "Couldn't save the model settings".to_string())
    }

    fn keys(&self) -> HashMap<String, String> {
        std::fs::read(&self.keys_path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn key(&self, slot: &str) -> Option<String> {
        self.keys().get(slot).filter(|k| !k.is_empty()).cloned()
    }

    /// Sets (or with `None` removes) the key of `slot`.
    pub fn set_key(&self, slot: &str, key: Option<&str>) -> Result<(), String> {
        let key = key.map(str::trim).filter(|k| !k.is_empty());
        if key.is_some_and(|k| !k.chars().all(|c| c.is_ascii_graphic())) {
            return Err(
                "That doesn't look like an API key. Paste it without spaces or line breaks".into(),
            );
        }
        let mut keys = self.keys();
        match key {
            Some(k) => keys.insert(slot.to_string(), k.to_string()),
            None => keys.remove(slot),
        };
        write_private(&self.keys_path, &serde_json::to_vec_pretty(&keys).unwrap())
            .map_err(|_| "Couldn't save the API key".to_string())
    }

    /// The last four characters of each saved key, so the settings can show which one is set.
    pub fn key_hints(&self) -> HashMap<String, String> {
        self.keys()
            .into_iter()
            .filter(|(_, k)| !k.is_empty())
            .map(|(slot, k)| {
                // A short key would be shown whole; show nothing of it instead.
                let tail: String = if k.chars().count() >= 12 {
                    let chars: Vec<char> = k.chars().collect();
                    chars[chars.len() - 4..].iter().collect()
                } else {
                    String::new()
                };
                (slot, tail)
            })
            .collect()
    }
}

/// Writes `bytes` readable by the owner only (on unix), replacing the file atomically.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)
}

/// A resolved model backend, ready to call. `Debug` leaves the key out.
#[derive(Clone)]
pub enum Backend {
    ClaudeCode {
        path: PathBuf,
        model: String,
    },
    Codex {
        path: PathBuf,
        model: String,
    },
    Anthropic {
        base: String,
        key: String,
        model: String,
    },
    Openai {
        base: String,
        key: Option<String>,
        model: String,
    },
    /// The chosen backend, then its backups in order. `unready` says why the chosen one couldn't
    /// even be set up (it is then missing from `links`).
    Chain {
        chosen: String,
        unready: Option<String>,
        links: Vec<(String, Backend)>,
        switch: Arc<Mutex<Switch>>,
    },
}

impl std::fmt::Debug for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Backend::ClaudeCode { path, model } => {
                write!(f, "ClaudeCode({}, {model})", path.display())
            }
            Backend::Codex { path, model } => write!(f, "Codex({}, {model})", path.display()),
            Backend::Anthropic { base, model, .. } => write!(f, "Anthropic({base}, {model})"),
            Backend::Openai { base, model, .. } => write!(f, "Openai({base}, {model})"),
            Backend::Chain { links, .. } => f.debug_list().entries(links).finish(),
        }
    }
}

/// Why the current backend can't run, for the notice at the top of the app.
#[derive(Serialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Problem {
    Missing,
    LoggedOut,
    NoKey,
    NoModel,
    BadUrl,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub provider: Provider,
    pub label: String,
    /// The model calls use, or empty for the CLI's own default (Codex).
    pub model: String,
    /// OpenAI-compatible preset id, so the app can show that service's mark.
    pub preset: String,
    pub problem: Option<Problem>,
    /// The backup answering calls for now, if the chosen backend stopped.
    pub backup: Option<Backup>,
    /// How many saved backups are set up and could take over.
    pub backups: usize,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub provider: Provider,
    pub label: String,
    pub model: String,
    pub preset: String,
    /// Why the chosen backend didn't answer.
    pub reason: String,
}

const CODEX_MISSING: &str = "Couldn't find the Codex CLI. Install Codex or the ChatGPT app, or pick another model in Model settings";
const NO_KEY: &str = "Add an API key in Model settings";
const NO_MODEL: &str = "Pick a model in Model settings";
const BAD_URL: &str = "Add the API address in Model settings. It must start with https:// (plain http:// only for this computer or without a key)";

pub fn label(s: &Settings) -> String {
    match s.provider {
        Provider::ClaudeCode => "Claude Code".into(),
        Provider::Codex => "Codex".into(),
        Provider::Anthropic => "Anthropic API".into(),
        Provider::Openai => s.preset().name.to_string(),
    }
}

/// Builds the backend for `s` without any network or login check. With backups saved, it is a
/// [`Backend::Chain`] of the chosen backend and every backup that can be set up.
pub fn backend(s: &Settings, store: &Store) -> Result<Backend, String> {
    let chosen = s.slot();
    let first = backend_one(s, store);
    let backups: Vec<(String, Backend)> = s
        .backups
        .iter()
        .filter(|slot| **slot != chosen)
        .filter_map(|slot| {
            let b = backend_one(&s.with_slot(slot)?, store).ok()?;
            Some((slot.clone(), b))
        })
        .collect();
    if backups.is_empty() {
        return first;
    }
    let (links, unready) = match first {
        Ok(b) => (vec![(chosen.clone(), b)], None),
        Err(e) => (Vec::new(), Some(e)),
    };
    Ok(Backend::Chain {
        chosen,
        unready,
        links: links.into_iter().chain(backups).collect(),
        switch: store.switch.clone(),
    })
}

/// An error after which the next backup should take over: a usage or rate limit, no credit, or
/// a backend that isn't installed, logged in or set up. Not a network hiccup or a bad reply.
pub fn switchable(err: &str) -> bool {
    let e = err.to_lowercase();
    [
        "limit",
        "out of credit",
        "key was rejected",
        "logged in",
        "couldn't find the",
        "add an api key",
        "add the api address",
        "pick a model",
    ]
    .iter()
    .any(|k| e.contains(k))
        && !e.contains("length limit")
}

/// One model call on a backend, sending reply text to the given callback.
type Call<'a> = dyn FnMut(&Backend, &mut dyn FnMut(&str)) -> Result<String, String> + 'a;

/// Runs a call down the chain: a backend resting after a limit is skipped (except the last), and
/// one that fails with a [`switchable`] error before sending any text hands over to the next.
fn run_chain(
    chosen: &str,
    unready: &Option<String>,
    links: &[(String, Backend)],
    switch: &Mutex<Switch>,
    cancel: &AtomicBool,
    call: &mut Call<'_>,
    on_delta: &mut dyn FnMut(&str),
) -> Result<String, String> {
    let mut reason = unready.clone();
    for (i, (slot, backend)) in links.iter().enumerate() {
        let last = i + 1 == links.len();
        if !last {
            if let Some(why) = switch.lock().unwrap().resting(slot) {
                reason.get_or_insert(why);
                continue;
            }
        }
        let mut sent = false;
        let result = call(backend, &mut |d: &str| {
            sent = true;
            on_delta(d);
        });
        match result {
            Ok(text) => {
                let mut sw = switch.lock().unwrap();
                sw.resting.remove(slot);
                sw.serving =
                    (slot != chosen).then(|| (slot.clone(), reason.clone().unwrap_or_default()));
                return Ok(text);
            }
            Err(e) if !last && !sent && !cancel.load(Ordering::Relaxed) && switchable(&e) => {
                switch
                    .lock()
                    .unwrap()
                    .resting
                    .insert(slot.clone(), (Instant::now() + REST, e.clone()));
                reason.get_or_insert(e);
            }
            Err(e) => return Err(e),
        }
    }
    Err(unready
        .clone()
        .unwrap_or_else(|| "No model is set up".to_string()))
}

/// The chosen backend alone, without backups.
pub fn backend_one(s: &Settings, store: &Store) -> Result<Backend, String> {
    let model = s.model();
    if matches!(s.provider, Provider::ClaudeCode | Provider::Codex) && !model_ok(&model) {
        return Err(BAD_MODEL.into());
    }
    match s.provider {
        Provider::ClaudeCode => {
            let path =
                translate::find_claude().ok_or_else(|| translate::NOT_INSTALLED.to_string())?;
            Ok(Backend::ClaudeCode { path, model })
        }
        Provider::Codex => {
            let path = find_codex().ok_or_else(|| CODEX_MISSING.to_string())?;
            Ok(Backend::Codex { path, model })
        }
        Provider::Anthropic => {
            let key = store.key(&s.slot()).ok_or_else(|| NO_KEY.to_string())?;
            Ok(Backend::Anthropic {
                base: ANTHROPIC_BASE.into(),
                key,
                model,
            })
        }
        Provider::Openai => {
            let base = s.openai_base();
            let key = store.key(&s.slot());
            if !base_allowed(&base, key.is_some()) {
                return Err(BAD_URL.into());
            }
            if key.is_none() && s.needs_key() {
                return Err(NO_KEY.into());
            }
            if model.is_empty() {
                return Err(NO_MODEL.into());
            }
            Ok(Backend::Openai { base, key, model })
        }
    }
}

/// Whether the current backend can run. CLIs are checked for install and login (no model call).
pub fn status(s: &Settings, store: &Store) -> Status {
    let problem = match s.provider {
        Provider::ClaudeCode => {
            let c = translate::claude_status();
            (!c.installed)
                .then_some(Problem::Missing)
                .or((!c.logged_in).then_some(Problem::LoggedOut))
        }
        Provider::Codex => match find_codex() {
            None => Some(Problem::Missing),
            Some(p) => (!codex_logged_in(&p)).then_some(Problem::LoggedOut),
        },
        Provider::Anthropic => store.key(&s.slot()).is_none().then_some(Problem::NoKey),
        Provider::Openai => {
            if !base_allowed(&s.openai_base(), store.key(&s.slot()).is_some()) {
                Some(Problem::BadUrl)
            } else if s.needs_key() && store.key(&s.slot()).is_none() {
                Some(Problem::NoKey)
            } else if s.model().is_empty() {
                Some(Problem::NoModel)
            } else {
                None
            }
        }
    };
    Status {
        provider: s.provider,
        label: label(s),
        model: s.model(),
        preset: s.preset.clone(),
        problem,
        backup: store.serving(s),
        backups: s
            .backups
            .iter()
            .filter(|slot| **slot != s.slot())
            .filter_map(|slot| s.with_slot(slot))
            .filter(|b| backend_one(b, store).is_ok())
            .count(),
    }
}

impl Backend {
    /// One reply for `input` under `system`.
    pub fn complete(
        &self,
        system: &str,
        input: &str,
        cancel: &AtomicBool,
    ) -> Result<String, String> {
        match self {
            Backend::ClaudeCode { path, model } => {
                translate::run_claude_with(path, model, system, input, cancel)
            }
            Backend::Chain {
                chosen,
                unready,
                links,
                switch,
            } => run_chain(
                chosen,
                unready,
                links,
                switch,
                cancel,
                &mut |b, _| b.complete(system, input, cancel),
                &mut |_| {},
            ),
            _ => self.stream(system, input, cancel, |_| {}),
        }
    }

    /// Like [`Backend::complete`], calling `on_delta` with each piece as it arrives. Codex
    /// delivers its answer in one piece at the end.
    pub fn stream(
        &self,
        system: &str,
        input: &str,
        cancel: &AtomicBool,
        mut on_delta: impl FnMut(&str),
    ) -> Result<String, String> {
        match self {
            Backend::Chain {
                chosen,
                unready,
                links,
                switch,
            } => run_chain(
                chosen,
                unready,
                links,
                switch,
                cancel,
                &mut |b, f| b.stream(system, input, cancel, f),
                &mut on_delta,
            ),
            Backend::ClaudeCode { path, model } => {
                translate::run_claude_stream(path, model, system, input, cancel, on_delta)
            }
            Backend::Codex { path, model } => {
                run_codex(path, model, system, input, cancel, on_delta)
            }
            Backend::Anthropic { base, key, model } => {
                let body = serde_json::json!({
                    "model": model,
                    "max_tokens": ANTHROPIC_MAX_TOKENS,
                    "system": system,
                    "messages": [{ "role": "user", "content": input }],
                    "stream": true,
                });
                let req = |http: &reqwest::blocking::Client| {
                    http.post(format!("{base}/v1/messages"))
                        .header("x-api-key", key)
                        .header("anthropic-version", "2023-06-01")
                        .header(reqwest::header::CONTENT_TYPE, "application/json")
                        .body(body.to_string())
                };
                run_sse(req, parse_anthropic, true, cancel, on_delta)
            }
            Backend::Openai { base, key, model } => {
                let body = serde_json::json!({
                    "model": model,
                    "messages": [
                        { "role": "system", "content": system },
                        { "role": "user", "content": input },
                    ],
                    "stream": true,
                });
                let req = |http: &reqwest::blocking::Client| {
                    let mut r = http
                        .post(format!("{base}/chat/completions"))
                        .header(reqwest::header::CONTENT_TYPE, "application/json")
                        .body(body.to_string());
                    if let Some(k) = key {
                        r = r.bearer_auth(k);
                    }
                    if base.starts_with("https://openrouter.ai/") {
                        r = r
                            .header("HTTP-Referer", "https://sionic-ai.github.io/openpaper/")
                            .header("X-Title", "OpenPaper");
                    }
                    r
                };
                run_sse(req, parse_openai, false, cancel, on_delta)
            }
        }
    }
}

// ---- HTTP APIs ------------------------------------------------------------------------------

fn http() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("openpaper/", env!("CARGO_PKG_VERSION")))
        // An API key must never follow a redirect to another host.
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(translate::TIMEOUT)
        .build()
        .map_err(|_| "Couldn't start a network connection".to_string())
}

/// One parsed `data:` line of a server-sent event stream.
#[derive(Debug, PartialEq)]
enum Event {
    Text(String),
    Error(String),
    Done,
    Other,
}

fn parse_anthropic(data: &str) -> Event {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
        return Event::Other;
    };
    match v["type"].as_str() {
        Some("content_block_delta") if v["delta"]["type"] == "text_delta" => {
            Event::Text(v["delta"]["text"].as_str().unwrap_or_default().to_string())
        }
        Some("message_delta") if v["delta"]["stop_reason"] == "max_tokens" => {
            Event::Error(CUT_OFF.into())
        }
        Some("message_stop") => Event::Done,
        Some("error") => Event::Error(
            v["error"]["message"]
                .as_str()
                .unwrap_or("error")
                .to_string(),
        ),
        _ => Event::Other,
    }
}

fn parse_openai(data: &str) -> Event {
    if data.trim() == "[DONE]" {
        return Event::Done;
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
        return Event::Other;
    };
    if let Some(msg) = v["error"]["message"].as_str() {
        return Event::Error(msg.to_string());
    }
    if v["choices"][0]["finish_reason"] == "length" {
        return Event::Error(CUT_OFF.into());
    }
    match v["choices"][0]["delta"]["content"].as_str() {
        Some(t) if !t.is_empty() => Event::Text(t.to_string()),
        _ => Event::Other,
    }
}

const CUT_OFF: &str = "the reply was cut off at the model's length limit";

const DROPPED: &str = "the connection dropped before the reply finished";

/// A short, plain message for a failed API request.
fn http_error(status: u16, body: &str) -> String {
    let detail = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v["error"]["message"]
                .as_str()
                .or(v["message"].as_str())
                .map(str::to_string)
        })
        .map(|m| {
            let m: String = m.chars().take(200).collect();
            format!(": {m}")
        })
        .unwrap_or_default();
    match status {
        401 | 403 => format!("The API key was rejected. Check it in Model settings{detail}"),
        402 => format!("The API account is out of credit{detail}"),
        404 => format!("The API address or model wasn't found. Check Model settings{detail}"),
        429 => format!("The API rate limit was reached. Wait a moment and try again{detail}"),
        _ => format!("The API request failed (HTTP {status}){detail}"),
    }
}

/// Sends the request (retrying rate limits and overloads twice), then reads the event stream.
/// `needs_done`: the stream always ends with a final event (Anthropic), so ending without one
/// means the connection dropped.
fn run_sse(
    request: impl Fn(&reqwest::blocking::Client) -> reqwest::blocking::RequestBuilder,
    parse: fn(&str) -> Event,
    needs_done: bool,
    cancel: &AtomicBool,
    mut on_delta: impl FnMut(&str),
) -> Result<String, String> {
    let http = http()?;
    let mut waits = [Duration::from_secs(2), Duration::from_secs(6)].into_iter();
    let response = loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Canceled".into());
        }
        let r = request(&http).send().map_err(|e| {
            if e.is_timeout() {
                "The API didn't answer in time".to_string()
            } else {
                "Couldn't reach the API. Check your connection and the address in Model settings"
                    .to_string()
            }
        })?;
        let code = r.status().as_u16();
        if r.status().is_success() {
            break r;
        }
        let retry = matches!(code, 429 | 500 | 502 | 503 | 529);
        match waits.next().filter(|_| retry) {
            Some(wait) => {
                let until = Instant::now() + wait;
                while Instant::now() < until {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Canceled".into());
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            None => return Err(http_error(code, &r.text().unwrap_or_default())),
        }
    };
    let (tx, rx) = mpsc::channel::<Event>();
    std::thread::spawn(move || {
        let mut read = 0usize;
        for line in std::io::BufReader::new(response).lines() {
            let Ok(line) = line else {
                let _ = tx.send(Event::Error(DROPPED.into()));
                return;
            };
            read += line.len();
            if read > translate::MAX_OUTPUT {
                let _ = tx.send(Event::Error("the reply was too long".into()));
                return;
            }
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let ev = parse(data.trim_start());
            if ev != Event::Other && tx.send(ev).is_err() {
                break;
            }
        }
    });
    let started = Instant::now();
    let mut text = String::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Canceled".into());
        }
        if started.elapsed() > translate::TIMEOUT {
            return Err("The API didn't finish in time".into());
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Event::Text(t)) => {
                text.push_str(&t);
                on_delta(&t);
            }
            Ok(Event::Error(m)) => {
                return Err(format!(
                    "The API stopped with an error: {}",
                    m.chars().take(200).collect::<String>()
                ));
            }
            Ok(Event::Done) => break,
            Ok(Event::Other) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            // OpenAI-compatible servers sometimes close the stream without `[DONE]`.
            Err(mpsc::RecvTimeoutError::Disconnected) if !needs_done => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(format!("The API stopped with an error: {DROPPED}"));
            }
        }
    }
    if text.trim().is_empty() {
        return Err("The API sent back an empty reply".into());
    }
    Ok(text)
}

/// Model ids the endpoint lists, sorted. Uses the saved key of the current slot.
/// One choice in the model picker. `name` and `note` are set when the source has them (Codex).
#[derive(Serialize, Debug, PartialEq)]
pub struct ModelOption {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl ModelOption {
    fn id(id: String) -> Self {
        ModelOption {
            id,
            name: None,
            note: None,
        }
    }
}

/// Models to pick from: the API's model list, or the models Codex lists for this login (read
/// from its local cache, no network). Claude Code has a fixed list in the app.
pub fn models(s: &Settings, store: &Store) -> Result<Vec<ModelOption>, String> {
    if s.provider == Provider::Codex {
        return Ok(codex_models());
    }
    let http = http()?;
    let key = store.key(&s.slot());
    let req = match s.provider {
        Provider::Anthropic => {
            let key = key.ok_or_else(|| NO_KEY.to_string())?;
            http.get(format!("{ANTHROPIC_BASE}/v1/models?limit=100"))
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
        }
        Provider::Openai => {
            let base = s.openai_base();
            if !base_allowed(&base, key.is_some()) {
                return Err(BAD_URL.into());
            }
            let mut r = http.get(format!("{base}/models"));
            if let Some(k) = key {
                r = r.bearer_auth(k);
            }
            r
        }
        _ => return Ok(Vec::new()),
    };
    let r = req
        .timeout(Duration::from_secs(20))
        .send()
        .map_err(|_| "Couldn't reach the API. Check your connection and the address".to_string())?;
    let code = r.status().as_u16();
    let body = r.text().unwrap_or_default();
    if !(200..300).contains(&code) {
        return Err(http_error(code, &body));
    }
    let mut ids = model_ids(&body);
    ids.sort();
    ids.dedup();
    Ok(ids.into_iter().map(ModelOption::id).collect())
}

/// `{"data": [{"id": ...}]}` (OpenAI, Anthropic, OpenRouter) or `{"models": [{"name": ...}]}`.
/// Skips entries a gateway marks as not for chat (`endpoints` without `chat_completions`, such as
/// image or embedding models) or as no longer served (`status` other than `active`).
fn model_ids(body: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    let list = v["data"].as_array().or(v["models"].as_array());
    list.map(|items| {
        items
            .iter()
            .filter(|m| {
                m["endpoints"]
                    .as_array()
                    .is_none_or(|e| e.iter().any(|e| e == "chat_completions"))
            })
            .filter(|m| m["status"].as_str().is_none_or(|s| s == "active"))
            .filter_map(|m| m["id"].as_str().or(m["name"].as_str()))
            .map(|id| id.trim_start_matches("models/").to_string())
            .collect()
    })
    .unwrap_or_default()
}

// ---- Codex CLI ------------------------------------------------------------------------------

fn codex_home() -> Option<PathBuf> {
    if let Some(v) = std::env::var_os("CODEX_HOME").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(v));
    }
    let home = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(home).map(|h| PathBuf::from(h).join(".codex"))
}

/// Models Codex shows in its own picker, from `models_cache.json` in the Codex home. Empty when
/// Codex hasn't fetched the list yet; the picker then offers only the default and a typed name.
fn codex_models() -> Vec<ModelOption> {
    codex_home()
        .and_then(|h| std::fs::read_to_string(h.join("models_cache.json")).ok())
        .map(|body| codex_model_list(&body))
        .unwrap_or_default()
}

fn codex_model_list(body: &str) -> Vec<ModelOption> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    let Some(items) = v["models"].as_array() else {
        return Vec::new();
    };
    let text = |m: &serde_json::Value, k: &str| {
        m[k].as_str()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(String::from)
    };
    items
        .iter()
        .filter(|m| m["visibility"].as_str() == Some("list"))
        .filter_map(|m| {
            let id = text(m, "slug")?;
            model_ok(&id).then(|| ModelOption {
                name: text(m, "display_name"),
                note: text(m, "description"),
                id,
            })
        })
        .collect()
}

pub fn find_codex() -> Option<PathBuf> {
    codex_candidates()
        .into_iter()
        .find(|p| translate::is_program(p))
}

#[cfg(not(windows))]
fn codex_candidates() -> Vec<PathBuf> {
    let mut c: Vec<PathBuf> = Vec::new();
    if let Some(h) = std::env::var_os("HOME").map(PathBuf::from) {
        c.push(h.join(".local/bin/codex"));
        c.push(h.join(".npm-global/bin/codex"));
    }
    c.push("/opt/homebrew/bin/codex".into());
    c.push("/usr/local/bin/codex".into());
    if let Some(path) = std::env::var_os("PATH") {
        c.extend(std::env::split_paths(&path).map(|d| d.join("codex")));
    }
    // The ChatGPT desktop app ships the Codex CLI.
    c.push("/Applications/ChatGPT.app/Contents/Resources/codex-cli/bin/codex".into());
    c
}

#[cfg(windows)]
fn codex_candidates() -> Vec<PathBuf> {
    // `codex.exe`, or npm's `codex.cmd`. The prompt goes over stdin and every argument is fixed
    // or a checked model name, so running the `.cmd` through `cmd.exe` is safe.
    let mut c: Vec<PathBuf> = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            c.push(d.join("codex.exe"));
            c.push(d.join("codex.cmd"));
        }
    }
    if let Some(a) = std::env::var_os("APPDATA").map(PathBuf::from) {
        c.push(a.join(r"npm\codex.cmd"));
    }
    c
}

/// Same cleared environment as the Claude CLI, plus `CODEX_HOME` when the user moved it.
fn codex_command(codex: &Path, tmp: Option<&Path>) -> Command {
    let mut cmd = Command::new(codex);
    translate::scrub_env(&mut cmd, codex, tmp);
    if let Some(v) = std::env::var_os("CODEX_HOME") {
        cmd.env("CODEX_HOME", v);
    }
    cmd
}

/// `codex login status` exits 0 when logged in. No model call.
fn codex_logged_in(codex: &Path) -> bool {
    codex_command(codex, None)
        .args(["login", "status"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Codex features that give the agent tools: a shell, code execution, file patches, ChatGPT
/// connectors, plugins, sub-agents, a browser, computer use. Paper text is untrusted, so every
/// one this Codex version has is turned off, leaving a plain model call.
const CODEX_TOOLS: &[&str] = &[
    "shell_tool",
    "unified_exec",
    "code_mode_host",
    "apps",
    "plugins",
    "remote_plugin",
    "multi_agent",
    "collaboration_modes",
    "image_generation",
    "browser_use",
    "browser_use_external",
    "in_app_browser",
    "computer_use",
    "hooks",
    "skill_search",
    "view_image",
    "tool_suggest",
    "goals",
    "sleep_tool",
];

const CODEX_UNSAFE: &str = "This Codex version's tools can't be turned off, so OpenPaper won't send it paper text. Update Codex, or pick another model in Model settings";

type Features = std::collections::HashSet<String>;

/// Feature names this Codex knows (`codex features list`), cached per binary. Unknown names in
/// `--disable` make Codex refuse to start.
fn codex_features(codex: &Path) -> Option<Features> {
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Features>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(f) = cache.lock().unwrap().get(codex) {
        return Some(f.clone());
    }
    let out = codex_command(codex, None)
        .args(["features", "list"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let names: Features = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_string)
        .collect();
    cache
        .lock()
        .unwrap()
        .insert(codex.to_path_buf(), names.clone());
    Some(names)
}

/// `--disable` arguments for every tool feature this Codex has. `None` when even its shell
/// can't be named, so it can't be locked down.
fn codex_lockdown(features: &Features) -> Option<Vec<String>> {
    if !features.contains("shell_tool") {
        return None;
    }
    Some(
        CODEX_TOOLS
            .iter()
            .filter(|f| features.contains(**f))
            .flat_map(|f| ["--disable".to_string(), (*f).to_string()])
            .collect(),
    )
}

/// Codex has no system prompt option, so the instructions go first in the prompt.
fn codex_prompt(system: &str, input: &str) -> String {
    format!("<instructions>\n{system}\n</instructions>\n\n{input}")
}

#[derive(Debug, PartialEq)]
enum CodexLine {
    Message(String),
    Failed(String),
    Other,
}

fn parse_codex_line(line: &str) -> CodexLine {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return CodexLine::Other;
    };
    match v["type"].as_str() {
        Some("item.completed") if v["item"]["type"] == "agent_message" => {
            CodexLine::Message(v["item"]["text"].as_str().unwrap_or_default().to_string())
        }
        Some("turn.failed") => CodexLine::Failed(
            v["error"]["message"]
                .as_str()
                .unwrap_or("turn failed")
                .to_string(),
        ),
        // Top-level `error` events include retries Codex recovers from; only a failed turn counts.
        _ => CodexLine::Other,
    }
}

const CODEX_FAILED: &str =
    "Codex failed. Check that you are logged in (run `codex login`) and within your usage limit";

/// `codex exec` in an empty private folder: read-only sandbox, no user config or rules, no
/// saved session. The last agent message is the reply.
fn run_codex(
    codex: &Path,
    model: &str,
    system: &str,
    input: &str,
    cancel: &AtomicBool,
    mut on_delta: impl FnMut(&str),
) -> Result<String, String> {
    let lockdown = codex_features(codex)
        .as_ref()
        .and_then(codex_lockdown)
        .ok_or_else(|| CODEX_UNSAFE.to_string())?;
    let dir = translate::private_dir()?;
    let mut cmd = codex_command(codex, Some(&dir));
    cmd.current_dir(&dir)
        .args(lockdown)
        .args(["-c", "web_search=\"disabled\""])
        .args([
            "exec",
            "--json",
            "--ephemeral",
            "--skip-git-repo-check",
            "--ignore-user-config",
            "--ignore-rules",
            "--sandbox",
            "read-only",
            "--color",
            "never",
        ]);
    cmd.arg("--cd").arg(&dir);
    if !model.is_empty() {
        cmd.args(["--model", model]);
    }
    cmd.arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let result = (|| {
        let mut child = cmd.spawn().map_err(|_| "Couldn't run Codex".to_string())?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let payload = codex_prompt(system, input).into_bytes();
        let writer = std::thread::spawn(move || {
            let _ = stdin.write_all(&payload);
        });
        let stdout = child.stdout.take().expect("piped stdout");
        let (tx, rx) = mpsc::channel::<CodexLine>();
        let reader = std::thread::spawn(move || {
            let mut read = 0usize;
            for line in std::io::BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                read += line.len();
                if read > translate::MAX_OUTPUT {
                    break;
                }
                let parsed = parse_codex_line(&line);
                if parsed != CodexLine::Other && tx.send(parsed).is_err() {
                    break;
                }
            }
        });
        let started = Instant::now();
        let mut last: Option<String> = None;
        let mut failed: Option<String> = None;
        loop {
            if cancel.load(Ordering::Relaxed) || started.elapsed() > translate::TIMEOUT {
                let _ = child.kill();
                let _ = child.wait();
                let _ = writer.join();
                let _ = reader.join();
                return Err(if cancel.load(Ordering::Relaxed) {
                    "Canceled".into()
                } else {
                    "Codex timed out".into()
                });
            }
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(CodexLine::Message(m)) => last = Some(m),
                Ok(CodexLine::Failed(m)) => failed = Some(m),
                Ok(CodexLine::Other) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        let status = child
            .wait()
            .map_err(|_| "Couldn't check on Codex".to_string())?;
        let _ = writer.join();
        let _ = reader.join();
        if let Some(m) = failed {
            return Err(format!(
                "Codex failed: {}",
                m.chars().take(200).collect::<String>()
            ));
        }
        match last.filter(|m| !m.trim().is_empty()) {
            Some(m) if status.success() => {
                on_delta(&m);
                Ok(m)
            }
            _ => Err(CODEX_FAILED.into()),
        }
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn codex_cache_lists_only_visible_models() {
        let body = r#"{"models":[
            {"slug":"gpt-a","display_name":"GPT-A","visibility":"list","description":"Workhorse."},
            {"slug":"gpt-hidden","display_name":"Hidden","visibility":"hide"},
            {"slug":"bad name;rm","visibility":"list"},
            {"slug":"gpt-b","visibility":"list"}
        ]}"#;
        let list = super::codex_model_list(body);
        let ids: Vec<&str> = list.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["gpt-a", "gpt-b"]);
        assert_eq!(list[0].name.as_deref(), Some("GPT-A"));
        assert_eq!(list[0].note.as_deref(), Some("Workhorse."));
        assert_eq!(list[1].name, None);
        assert!(super::codex_model_list("not json").is_empty());
    }

    use super::*;

    #[test]
    fn settings_pick_slots_and_default_models() {
        let mut s = Settings::default();
        assert_eq!(s.provider, Provider::ClaudeCode);
        assert_eq!(
            (s.slot(), s.model()),
            ("claude-code".to_string(), "sonnet".to_string())
        );
        s.provider = Provider::Anthropic;
        assert_eq!(s.model(), ANTHROPIC_MODEL);
        s.provider = Provider::Openai;
        s.preset = "openrouter".into();
        assert_eq!(s.slot(), "openai:openrouter");
        assert_eq!(s.model(), "");
        s.models.insert(
            "openai:openrouter".into(),
            " anthropic/claude-sonnet ".into(),
        );
        assert_eq!(s.model(), "anthropic/claude-sonnet");
        assert_eq!(s.openai_base(), "https://openrouter.ai/api/v1");
        s.preset = "custom".into();
        s.base_url = "https://llm.example.com/v1/".into();
        assert_eq!(s.openai_base(), "https://llm.example.com/v1");
        s.preset = "ollama".into();
        assert!(!s.needs_key());
        s.preset = "custom".into();
        assert!(!s.needs_key());
        s.preset = "no-such-preset".into();
        assert_eq!(s.openai_base(), "https://api.openai.com/v1");
    }

    #[test]
    fn old_or_partial_settings_files_still_load() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(
            s,
            Settings {
                preset: "openai".into(),
                ..Settings::default()
            }
        );
        let s: Settings = serde_json::from_str(r#"{"provider":"codex"}"#).unwrap();
        assert_eq!(s.provider, Provider::Codex);
    }

    #[test]
    fn keys_only_travel_over_https_or_to_this_computer() {
        assert!(base_allowed("https://api.openai.com/v1", true));
        assert!(base_allowed("http://localhost:11434/v1", true));
        assert!(base_allowed("http://127.0.0.1:8000/v1", true));
        assert!(!base_allowed("http://api.example.com/v1", true));
        assert!(!base_allowed("http://localhost.evil.example/v1", true));
        assert!(!base_allowed("http://localhost@evil.example/v1", true));
        // A LAN server without a key is fine; with a key it must be https.
        assert!(base_allowed("http://192.168.0.9:11434/v1", false));
        assert!(!base_allowed("http://192.168.0.9:11434/v1", true));
        assert!(!base_allowed("ftp://example.com", false));
        assert!(!base_allowed("", false));
    }

    #[test]
    fn model_names_stay_plain() {
        for ok in [
            "sonnet",
            "gpt-5.5",
            "anthropic/claude-sonnet-5.5",
            "llama3:8b",
            "",
        ] {
            assert!(model_ok(ok), "{ok}");
        }
        assert!(!model_ok("--dangerously-bypass-approvals-and-sandbox"));
        assert!(!model_ok("x\" & calc"));
    }

    #[test]
    fn codex_tools_are_all_turned_off_or_codex_is_refused() {
        let have: Features = ["shell_tool", "apps", "fast_mode"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            codex_lockdown(&have).unwrap(),
            vec!["--disable", "shell_tool", "--disable", "apps"]
        );
        let old: Features = ["apps".to_string()].into_iter().collect();
        assert_eq!(codex_lockdown(&old), None);
    }

    #[test]
    fn debug_never_prints_a_key() {
        let b = Backend::Anthropic {
            base: ANTHROPIC_BASE.into(),
            key: "sk-ant-secret".into(),
            model: "m".into(),
        };
        assert!(!format!("{b:?}").contains("secret"));
    }

    #[test]
    fn reads_anthropic_and_openai_streams() {
        assert_eq!(
            parse_anthropic(
                r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}"#
            ),
            Event::Text("Hi".into())
        );
        assert_eq!(
            parse_anthropic(
                r#"{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"x"}}"#
            ),
            Event::Other
        );
        assert_eq!(parse_anthropic(r#"{"type":"message_stop"}"#), Event::Done);
        assert_eq!(
            parse_anthropic(
                r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#
            ),
            Event::Error("Overloaded".into())
        );
        assert_eq!(
            parse_openai(r#"{"choices":[{"delta":{"content":"Hi"}}]}"#),
            Event::Text("Hi".into())
        );
        assert_eq!(
            parse_openai(r#"{"choices":[{"delta":{"role":"assistant"}}]}"#),
            Event::Other
        );
        assert_eq!(parse_openai(" [DONE]"), Event::Done);
        assert_eq!(
            parse_openai(r#"{"error":{"message":"bad"}}"#),
            Event::Error("bad".into())
        );
        assert_eq!(
            parse_openai(r#"{"choices":[{"delta":{},"finish_reason":"length"}]}"#),
            Event::Error(CUT_OFF.into())
        );
        assert_eq!(
            parse_anthropic(r#"{"type":"message_delta","delta":{"stop_reason":"max_tokens"}}"#),
            Event::Error(CUT_OFF.into())
        );
        assert_eq!(
            parse_anthropic(r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"}}"#),
            Event::Other
        );
    }

    #[test]
    fn reads_codex_events() {
        assert_eq!(
            parse_codex_line(
                r#"{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"{\"units\":[]}"}}"#
            ),
            CodexLine::Message(r#"{"units":[]}"#.into())
        );
        assert_eq!(
            parse_codex_line(r#"{"type":"item.completed","item":{"type":"reasoning","text":"x"}}"#),
            CodexLine::Other
        );
        assert_eq!(
            parse_codex_line(r#"{"type":"turn.failed","error":{"message":"usage limit"}}"#),
            CodexLine::Failed("usage limit".into())
        );
        assert_eq!(
            parse_codex_line(r#"{"type":"error","message":"Reconnecting... 1/5"}"#),
            CodexLine::Other
        );
        assert!(codex_prompt("SYS", "IN").starts_with("<instructions>\nSYS\n</instructions>"));
    }

    #[test]
    fn api_errors_are_short_and_plain() {
        assert!(
            http_error(401, r#"{"error":{"message":"invalid x-api-key"}}"#)
                .starts_with("The API key was rejected")
        );
        assert!(http_error(429, "").starts_with("The API rate limit"));
        assert_eq!(http_error(500, "oops"), "The API request failed (HTTP 500)");
        let long = format!(r#"{{"error":{{"message":"{}"}}}}"#, "x".repeat(500));
        assert!(http_error(400, &long).len() < 260);
    }

    #[test]
    fn lists_models_from_either_shape() {
        assert_eq!(
            model_ids(r#"{"data":[{"id":"b"},{"id":"a"}]}"#),
            vec!["b", "a"]
        );
        assert_eq!(
            model_ids(r#"{"models":[{"name":"models/gemini-x"}]}"#),
            vec!["gemini-x"]
        );
        assert!(model_ids("nope").is_empty());
    }

    #[test]
    fn model_list_skips_models_a_gateway_cannot_chat_with() {
        let body = r#"{"data":[
            {"id":"chat","endpoints":["chat_completions","responses"],"status":"active"},
            {"id":"responses-only","endpoints":["responses"],"status":"active"},
            {"id":"image","endpoints":["images_generations"],"status":"active"},
            {"id":"retired","endpoints":["chat_completions"],"status":"retired"},
            {"id":"plain"}
        ]}"#;
        assert_eq!(model_ids(body), vec!["chat", "plain"]);
    }

    fn fake(model: &str) -> Backend {
        Backend::Openai {
            base: "http://localhost:1/v1".into(),
            key: None,
            model: model.into(),
        }
    }

    fn model_of(b: &Backend) -> String {
        match b {
            Backend::Openai { model, .. } => model.clone(),
            _ => String::new(),
        }
    }

    /// Runs a chain of fake backends; `answer(model)` says what each one does.
    fn run(
        links: &[(String, Backend)],
        switch: &Mutex<Switch>,
        tried: &mut Vec<String>,
        answer: impl Fn(&str, &mut dyn FnMut(&str)) -> Result<String, String>,
    ) -> Result<String, String> {
        run_chain(
            "claude-code",
            &None,
            links,
            switch,
            &AtomicBool::new(false),
            &mut |b, f| {
                tried.push(model_of(b));
                answer(&model_of(b), f)
            },
            &mut |_| {},
        )
    }

    #[test]
    fn a_usage_limit_hands_over_to_the_next_backup_and_rests() {
        let links = vec![
            ("claude-code".to_string(), fake("a")),
            ("openai:opengateway".to_string(), fake("b")),
        ];
        let switch = Mutex::new(Switch::default());
        let limited = |m: &str, _: &mut dyn FnMut(&str)| match m {
            "a" => Err("Claude Code is at its usage limit: resets 5pm".to_string()),
            m => Ok(format!("from {m}")),
        };
        let mut tried = Vec::new();
        assert_eq!(run(&links, &switch, &mut tried, limited).unwrap(), "from b");
        assert_eq!(tried, ["a", "b"]);
        let serving = switch.lock().unwrap().serving.clone().unwrap();
        assert_eq!(serving.0, "openai:opengateway");
        assert!(serving.1.contains("usage limit"));
        // While it rests, the chosen backend isn't tried at all.
        let mut tried = Vec::new();
        run(&links, &switch, &mut tried, limited).unwrap();
        assert_eq!(tried, ["b"]);
        // Once the rest is over and it answers, it is back in use.
        switch.lock().unwrap().resting.clear();
        let mut tried = Vec::new();
        let ok = |m: &str, _: &mut dyn FnMut(&str)| Ok::<_, String>(format!("from {m}"));
        assert_eq!(run(&links, &switch, &mut tried, ok).unwrap(), "from a");
        assert_eq!(switch.lock().unwrap().serving, None);
    }

    #[test]
    fn other_failures_and_half_sent_replies_stay_with_the_backend() {
        let links = vec![
            ("claude-code".to_string(), fake("a")),
            ("codex".to_string(), fake("b")),
        ];
        let switch = Mutex::new(Switch::default());
        let mut tried = Vec::new();
        let bad = |_: &str, _: &mut dyn FnMut(&str)| Err::<String, _>("Couldn't read the reply".into());
        assert!(run(&links, &switch, &mut tried, bad).is_err());
        assert_eq!(tried, ["a"]);
        let mut tried = Vec::new();
        let half = |_: &str, f: &mut dyn FnMut(&str)| {
            f("part");
            Err::<String, _>("The API rate limit was reached".into())
        };
        assert!(run(&links, &switch, &mut tried, half).is_err());
        assert_eq!(tried, ["a"]);
    }

    #[test]
    fn the_last_backup_is_tried_even_while_resting() {
        let links = vec![
            ("claude-code".to_string(), fake("a")),
            ("codex".to_string(), fake("b")),
        ];
        let switch = Mutex::new(Switch::default());
        let far = Instant::now() + REST;
        for slot in ["claude-code", "codex"] {
            switch
                .lock()
                .unwrap()
                .resting
                .insert(slot.into(), (far, "limit".into()));
        }
        let mut tried = Vec::new();
        let err = run(&links, &switch, &mut tried, |_, _| {
            Err("Codex failed: usage limit".to_string())
        });
        assert_eq!(tried, ["b"]);
        assert!(err.unwrap_err().contains("Codex"));
    }

    #[test]
    fn switchable_errors() {
        for e in [
            "Claude Code is at its usage limit",
            "Claude Code failed. Check that you are logged in and within your usage limit",
            "The API rate limit was reached. Wait a moment and try again",
            "The API account is out of credit",
            "The API key was rejected. Check it in Model settings",
            NO_KEY,
            CODEX_MISSING,
            translate::NOT_INSTALLED,
        ] {
            assert!(switchable(e), "{e}");
        }
        for e in [
            "Couldn't reach the API. Check your connection",
            "Canceled",
            "Couldn't read the reply from Claude Code",
            "the reply was cut off at the model's length limit",
        ] {
            assert!(!switchable(e), "{e}");
        }
    }

    #[test]
    fn backups_build_a_chain_even_when_the_chosen_one_cannot_run() {
        let dir = std::env::temp_dir().join(format!("openpaper-chain-test-{}", std::process::id()));
        let store = Store::new(&dir);
        let mut s = Settings {
            provider: Provider::Anthropic,
            ..Settings::default()
        };
        s.models.insert("openai:ollama".into(), "llama".into());
        assert_eq!(backend(&s, &store).unwrap_err(), NO_KEY);
        s.backups = vec!["anthropic".into(), "openai:ollama".into(), "nope".into()];
        match backend(&s, &store).unwrap() {
            Backend::Chain { unready, links, .. } => {
                assert_eq!(unready.as_deref(), Some(NO_KEY));
                assert_eq!(links.len(), 1);
                assert_eq!(links[0].0, "openai:ollama");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(s.with_slot("openai:nope"), None);
        assert_eq!(s.with_slot("codex").unwrap().slot(), "codex");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn opengateway_works_with_a_key_and_no_pick() {
        let s = Settings {
            provider: Provider::Openai,
            preset: "opengateway".into(),
            ..Settings::default()
        };
        assert_eq!(s.slot(), "openai:opengateway");
        assert_eq!(s.openai_base(), "https://apis.opengateway.ai/v1");
        assert_eq!(s.model(), "anthropic/claude-sonnet-5-5");
        assert!(s.needs_key());
        assert_eq!(s.key_page(), Some("https://opengateway.ai/api-keys"));
        assert_eq!(label(&s), "OpenGateway");
        let ollama = Settings {
            preset: "ollama".into(),
            ..s.clone()
        };
        assert_eq!(ollama.key_page(), None);
        assert_eq!(Settings::default().key_page(), None);
    }

    #[test]
    fn keys_are_saved_apart_and_hinted() {
        let dir = std::env::temp_dir().join(format!("openpaper-llm-test-{}", std::process::id()));
        let store = Store::new(&dir);
        assert_eq!(store.key("anthropic"), None);
        store.set_key("anthropic", Some(" sk-ant-123456 ")).unwrap();
        assert_eq!(store.key("anthropic").as_deref(), Some("sk-ant-123456"));
        assert_eq!(
            store.key_hints().get("anthropic").map(String::as_str),
            Some("3456")
        );
        assert!(store.set_key("anthropic", Some("sk-ant 123")).is_err());
        store.set_key("short", Some("abcd")).unwrap();
        assert_eq!(store.key_hints().get("short").map(String::as_str), Some(""));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join("keys.json"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0);
        }
        store.set_key("anthropic", None).unwrap();
        assert_eq!(store.key("anthropic"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
