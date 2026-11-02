//! Papers and their translations as JSON files under the app data directory.

use crate::lang::Lang;
use crate::model::{Doc, Unit};
use crate::translate::Mode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub id: String,
    pub title: String,
    pub source: String,
    pub added_at: u64,
    pub units: usize,
}

/// A sidebar folder. Its first paper is the one still shown when the folder is folded.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Folder {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub papers: Vec<String>,
    #[serde(default)]
    pub open: bool,
}

const MAX_FOLDERS: usize = 200;

/// Drops bad ids and repeats, so each paper sits in at most one folder.
fn clean_folders(folders: Vec<Folder>) -> Vec<Folder> {
    let mut seen_folder = std::collections::HashSet::new();
    let mut seen_paper = std::collections::HashSet::new();
    folders
        .into_iter()
        .filter(|f| valid_id(&f.id) && seen_folder.insert(f.id.clone()))
        .take(MAX_FOLDERS)
        .map(|f| {
            let name: String = f.name.trim().chars().take(60).collect();
            Folder {
                name: if name.is_empty() {
                    "New folder".into()
                } else {
                    name
                },
                papers: f
                    .papers
                    .into_iter()
                    .filter(|p| valid_id(p) && seen_paper.insert(p.clone()))
                    .collect(),
                ..f
            }
        })
        .collect()
}

/// Bundle identifier the app had before it was renamed to OpenPaper.
const LEGACY_APP_ID: &str = "dev.rlaope.nanran";

/// Moves `<app data>/../dev.rlaope.nanran/papers` to `<app data>/papers` when the new library
/// holds no papers or folders yet (a dir created empty by an earlier start, a dev run or the
/// probe counts as absent and is replaced). Returns whether anything moved.
pub fn migrate_legacy(app_data: &Path) -> std::io::Result<bool> {
    let new = app_data.join("papers");
    let Some(old) = app_data
        .parent()
        .map(|p| p.join(LEGACY_APP_ID).join("papers"))
    else {
        return Ok(false);
    };
    if !old.is_dir() || has_library_data(&new) {
        return Ok(false);
    }
    if new.exists() {
        std::fs::remove_dir_all(&new)?;
    }
    std::fs::create_dir_all(app_data)?;
    std::fs::rename(&old, &new)?;
    Ok(true)
}

/// True when `dir` holds a paper (`*.meta.json`) or the folder list. Unreadable counts as true,
/// so nothing is ever deleted on doubt.
fn has_library_data(dir: &Path) -> bool {
    if !dir.exists() {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return true;
    };
    entries.into_iter().any(|e| {
        e.map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name == "folders.json" || name.ends_with(".meta.json")
        })
        .unwrap_or(true)
    })
}

pub struct Library {
    root: PathBuf,
    /// Serializes translation-file read-modify-write across worker threads.
    write: Mutex<()>,
}

/// File suffix of saved translations: `<id>.<suffix>.json`. Korean keeps the names it had
/// before other languages existed (`ko`, `easy`), so older libraries stay valid.
pub fn translation_suffix(mode: Mode, lang: Lang) -> String {
    match (lang, mode) {
        (Lang::Ko, Mode::Plain) => "ko".into(),
        (Lang::Ko, Mode::Easy) => "easy".into(),
        (l, Mode::Plain) => l.code().into(),
        (l, Mode::Easy) => format!("{}.easy", l.code()),
    }
}

/// File suffix of the saved summary post: `summary` for Korean, `<lang>.summary` otherwise.
pub fn summary_suffix(lang: Lang) -> String {
    match lang {
        Lang::Ko => "summary".into(),
        l => format!("{}.summary", l.code()),
    }
}

/// Every suffix a paper can have on disk, for every language.
fn all_suffixes() -> Vec<String> {
    let mut out: Vec<String> = ["doc", "meta", "style"].map(String::from).to_vec();
    for lang in Lang::ALL {
        out.push(translation_suffix(Mode::Plain, lang));
        out.push(translation_suffix(Mode::Easy, lang));
        out.push(summary_suffix(lang));
    }
    out
}

/// Last dot-segments a paper id may not end with, besides language codes: they are file
/// suffixes, so `X.ja` with suffix `easy` would name the same file as `X` with `ja.easy`.
const RESERVED_SEGMENTS: [&str; 5] = ["doc", "meta", "style", "easy", "summary"];

fn valid_id(id: &str) -> bool {
    let last = id
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    !id.is_empty()
        && id.len() <= 80
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !id.contains("..")
        && !id.ends_with('.')
        // Compared without case: the macOS file system usually ignores it.
        && !RESERVED_SEGMENTS.contains(&last.as_str())
        && Lang::from_code(&last).is_none()
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)
        .and_then(|_| std::fs::rename(&tmp, path))
        .map_err(|_| "Couldn't save".to_string())
}

impl Library {
    pub fn open(root: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            write: Mutex::new(()),
        })
    }

    fn path(&self, id: &str, suffix: &str) -> Result<PathBuf, String> {
        if !valid_id(id) {
            return Err("Invalid paper ID".into());
        }
        Ok(self.root.join(format!("{id}.{suffix}.json")))
    }

    pub fn save(&self, doc: &Doc) -> Result<Meta, String> {
        let meta_path = self.path(&doc.id, "meta")?;
        let added_at = std::fs::read(&meta_path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Meta>(&b).ok())
            .map(|m| m.added_at)
            .unwrap_or_else(|| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or_default()
            });
        let meta = Meta {
            id: doc.id.clone(),
            title: doc.title.clone(),
            source: doc.source.clone(),
            added_at,
            units: doc.units().len(),
        };
        write_atomic(
            &self.path(&doc.id, "doc")?,
            &serde_json::to_vec(doc).unwrap(),
        )?;
        write_atomic(&meta_path, &serde_json::to_vec(&meta).unwrap())?;
        Ok(meta)
    }

    pub fn list(&self) -> Vec<Meta> {
        let mut out: Vec<Meta> = std::fs::read_dir(&self.root)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".meta.json"))
            .filter_map(|e| std::fs::read(e.path()).ok())
            .filter_map(|b| serde_json::from_slice(&b).ok())
            .collect();
        out.sort_by_key(|m: &Meta| std::cmp::Reverse(m.added_at));
        out
    }

    pub fn doc(&self, id: &str) -> Result<Doc, String> {
        let bytes = std::fs::read(self.path(id, "doc")?)
            .map_err(|_| "Couldn't find the paper".to_string())?;
        serde_json::from_slice(&bytes).map_err(|_| "The saved paper is damaged".to_string())
    }

    /// Saved text for one mode and language, e.g. `<id>.ko.json` (Korean translation),
    /// `<id>.easy.json` (Korean easy explanation) or `<id>.ja.easy.json`. See [`translation_suffix`].
    pub fn translations(&self, id: &str, mode: Mode, lang: Lang) -> HashMap<u32, Vec<String>> {
        self.path(id, &translation_suffix(mode, lang))
            .ok()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn add_translations(
        &self,
        id: &str,
        mode: Mode,
        lang: Lang,
        units: &[Unit],
    ) -> Result<(), String> {
        let _guard = self.write.lock().unwrap();
        // A batch that finishes after the paper was removed must not leave a file behind.
        if !self.path(id, "doc")?.exists() {
            return Ok(());
        }
        let mut all = self.translations(id, mode, lang);
        for u in units {
            all.insert(u.id, u.sentences.clone());
        }
        write_atomic(
            &self.path(id, &translation_suffix(mode, lang))?,
            &serde_json::to_vec(&all).unwrap(),
        )
    }

    /// Writing-style version of the saved Korean translation (see `translate::STYLE`); 1 when
    /// unrecorded. Other languages have no older styles.
    pub fn style(&self, id: &str) -> u32 {
        self.path(id, "style")
            .ok()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(1)
    }

    pub fn set_style(&self, id: &str, style: u32) -> Result<(), String> {
        write_atomic(&self.path(id, "style")?, style.to_string().as_bytes())
    }

    /// The saved summary post in `lang` (see [`summary_suffix`]), if one was made.
    pub fn summary(&self, id: &str, lang: Lang) -> Option<String> {
        let bytes = std::fs::read(self.path(id, &summary_suffix(lang)).ok()?).ok()?;
        serde_json::from_slice::<String>(&bytes)
            .ok()
            .filter(|s| !s.trim().is_empty())
    }

    pub fn set_summary(&self, id: &str, lang: Lang, text: &str) -> Result<(), String> {
        write_atomic(
            &self.path(id, &summary_suffix(lang))?,
            &serde_json::to_vec(text).unwrap(),
        )
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let _guard = self.write.lock().unwrap();
        for suffix in all_suffixes() {
            let _ = std::fs::remove_file(self.path(id, &suffix)?);
        }
        let mut folders = self.folders();
        if folders.iter().any(|f| f.papers.iter().any(|p| p == id)) {
            for f in &mut folders {
                f.papers.retain(|p| p != id);
            }
            self.write_folders(&folders)?;
        }
        Ok(())
    }

    fn folders_path(&self) -> PathBuf {
        self.root.join("folders.json")
    }

    /// Sidebar folders in display order (`folders.json`); empty when there are none yet.
    pub fn folders(&self) -> Vec<Folder> {
        std::fs::read(self.folders_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Replaces every folder at once and returns what was kept.
    pub fn set_folders(&self, folders: Vec<Folder>) -> Result<Vec<Folder>, String> {
        let _guard = self.write.lock().unwrap();
        let folders = clean_folders(folders);
        self.write_folders(&folders)?;
        Ok(folders)
    }

    fn write_folders(&self, folders: &[Folder]) -> Result<(), String> {
        write_atomic(&self.folders_path(), &serde_json::to_vec(folders).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Builder;

    #[test]
    fn rejects_path_like_ids() {
        for id in ["../x", "a/b", "", "a..b"] {
            assert!(!valid_id(id), "{id}");
        }
        assert!(valid_id("arxiv-1706.03762v7"));
        assert!(valid_id("arxiv-hep-th_9901001"));
        assert!(valid_id("arxiv-1706.03762"));
        assert!(valid_id(&format!("pdf-{}", "0123456789abcdef".repeat(2))));
        assert!(valid_id("arxiv-2101.00001.v2x"));
    }

    #[test]
    fn rejects_ids_that_collide_with_file_suffixes() {
        for id in [
            "x.ja",
            "x.KO",
            "x.De",
            "x.easy",
            "x.summary",
            "x.doc",
            "x.meta",
            "x.style",
            "x.",
            "x.ja.EASY",
            "pt",
        ] {
            assert!(!valid_id(id), "{id}");
        }
        // Every on-disk name belongs to exactly one (valid id, suffix) pair.
        let ids = ["x", "x.y", "arxiv-1706.03762v7"];
        let mut seen = std::collections::HashSet::new();
        for id in ids {
            for suffix in all_suffixes() {
                assert!(seen.insert(format!("{id}.{suffix}").to_lowercase()));
            }
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("openpaper-lib-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn unit0() -> Unit {
        Unit {
            id: 0,
            sentences: vec!["하나.".into(), "둘.".into()],
        }
    }

    #[test]
    fn round_trips_docs_and_translations() {
        let dir = temp_dir("roundtrip");
        let lib = Library::open(dir.clone()).unwrap();
        let mut b = Builder::default();
        b.title = "T".into();
        b.para("One. Two.");
        let doc = b.finish("pdf-abc".into(), "a.pdf");
        lib.save(&doc).unwrap();
        assert_eq!(lib.list()[0].units, 1);
        lib.add_translations("pdf-abc", Mode::Plain, Lang::Ko, &[unit0()])
            .unwrap();
        assert_eq!(
            lib.translations("pdf-abc", Mode::Plain, Lang::Ko)[&0].len(),
            2
        );
        assert!(lib.translations("pdf-abc", Mode::Easy, Lang::Ko).is_empty());
        assert!(
            lib.translations("pdf-abc", Mode::Plain, Lang::Ja)
                .is_empty()
        );
        assert_eq!(lib.style("pdf-abc"), 1);
        lib.set_style("pdf-abc", 2).unwrap();
        assert_eq!(lib.style("pdf-abc"), 2);
        let f = |id: &str, papers: &[&str]| Folder {
            id: id.into(),
            name: format!("  {id}  "),
            papers: papers.iter().map(|p| p.to_string()).collect(),
            open: false,
        };
        let kept = lib
            .set_folders(vec![
                f("a", &["pdf-abc", "x"]),
                f("b", &["pdf-abc", "../y"]),
                f("a", &[]),
            ])
            .unwrap();
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].name, "a");
        assert_eq!(kept[1].papers, Vec::<String>::new());
        assert_eq!(lib.folders(), kept);
        lib.remove("pdf-abc").unwrap();
        assert!(lib.list().is_empty());
        assert_eq!(lib.folders()[0].papers, vec!["x".to_string()]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn empty_folder_name_gets_default() {
        let kept = clean_folders(vec![Folder {
            id: "a".into(),
            name: "   ".into(),
            papers: vec![],
            open: false,
        }]);
        assert_eq!(kept[0].name, "New folder");
    }

    #[test]
    fn file_names_per_language() {
        let dir = temp_dir("names");
        let lib = Library::open(dir.clone()).unwrap();
        let name = |mode, lang| format!("x.{}.json", translation_suffix(mode, lang));
        // Korean keeps the names it had before other languages existed.
        assert_eq!(name(Mode::Plain, Lang::Ko), "x.ko.json");
        assert_eq!(name(Mode::Easy, Lang::Ko), "x.easy.json");
        assert_eq!(summary_suffix(Lang::Ko), "summary");
        assert_eq!(name(Mode::Plain, Lang::Ja), "x.ja.json");
        assert_eq!(name(Mode::Easy, Lang::De), "x.de.easy.json");
        assert_eq!(summary_suffix(Lang::Zh), "zh.summary");
        // Every suffix is distinct, so no language can read another's file.
        let all = all_suffixes();
        let unique: std::collections::HashSet<_> = all.iter().collect();
        assert_eq!(unique.len(), all.len());

        let mut b = Builder::default();
        b.para("One. Two.");
        lib.save(&b.finish("x".into(), "a.pdf")).unwrap();
        lib.add_translations("x", Mode::Plain, Lang::Ja, &[unit0()])
            .unwrap();
        lib.set_summary("x", Lang::Es, "post").unwrap();
        assert!(dir.join("x.ja.json").is_file());
        assert!(dir.join("x.es.summary.json").is_file());
        assert_eq!(lib.summary("x", Lang::Es).as_deref(), Some("post"));
        assert_eq!(lib.summary("x", Lang::Ko), None);
        assert!(lib.translations("x", Mode::Plain, Lang::Ko).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn no_translation_file_after_remove() {
        let dir = temp_dir("late");
        let lib = Library::open(dir.clone()).unwrap();
        let mut b = Builder::default();
        b.para("One. Two.");
        let doc = b.finish("p2".into(), "a.pdf");
        lib.save(&doc).unwrap();
        lib.remove("p2").unwrap();
        // A batch still in flight finishes after the removal.
        lib.add_translations("p2", Mode::Plain, Lang::Ja, &[unit0()])
            .unwrap();
        assert!(!dir.join("p2.ja.json").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn remove_deletes_every_language() {
        let dir = temp_dir("remove");
        let lib = Library::open(dir.clone()).unwrap();
        for suffix in all_suffixes() {
            std::fs::write(dir.join(format!("p1.{suffix}.json")), "{}").unwrap();
        }
        // Another paper whose id starts with the same text must survive.
        std::fs::write(dir.join("p1x.ja.json"), "{}").unwrap();
        lib.remove("p1").unwrap();
        let left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, ["p1x.ja.json"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn migrates_the_legacy_library_once() {
        let root = temp_dir("migrate");
        let old = root.join("dev.rlaope.nanran/papers");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("a.meta.json"), "{}").unwrap();
        let app_data = root.join("dev.rlaope.openpaper");
        assert!(migrate_legacy(&app_data).unwrap());
        assert!(app_data.join("papers/a.meta.json").is_file());
        assert!(!old.exists());
        std::fs::remove_dir_all(root.join("dev.rlaope.openpaper")).unwrap();
        // An empty new library, e.g. made by a dev run, does not block the move.
        let old = root.join("dev.rlaope.nanran/papers");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("folders.json"), "[]").unwrap();
        std::fs::create_dir_all(app_data.join("papers")).unwrap();
        std::fs::write(app_data.join("papers/stray.tmp"), "").unwrap();
        assert!(migrate_legacy(&app_data).unwrap());
        assert!(app_data.join("papers/folders.json").is_file());
        assert!(!app_data.join("papers/stray.tmp").exists());
        assert!(!old.exists());
        // An existing new library is never replaced.
        std::fs::create_dir_all(&old).unwrap();
        assert!(!migrate_legacy(&app_data).unwrap());
        assert!(old.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
