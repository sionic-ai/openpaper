//! Command-line check of import and translation without the GUI.
//!
//! openpaper-probe parse <arxiv-id | file.pdf>            print block statistics
//! openpaper-probe translate <arxiv-id | file.pdf> [n]    translate the first n units (default 6)
//! openpaper-probe dump <arxiv-id | file.pdf>             print every block on one line
//! openpaper-probe import <arxiv-id | file.pdf> <dir> [n] save into a library dir with n translated units
//! openpaper-probe search <query> [start]                 run an arXiv search
//! openpaper-probe ask <request>                          run the AI paper finder
//! openpaper-probe claude -                               show Claude Code install/login state
//! openpaper-probe update                                 compare with the latest GitHub release
//! openpaper-probe askpaper <doc-id> <question> [quote]   ask about a paper in the app's library
//! openpaper-probe summary <doc-id> [dir]               write the summary post (dir: a library copy)
//! openpaper-probe redo <doc-id> <dir>                    retranslate a saved paper in the current style
//!
//! openpaper-probe selfupdate <path to .app/Contents/MacOS/openpaper>  run the in-app updater
//!
//! Set OPENPAPER_MODE=easy to run translate/job/import as the easy explanation instead.
//! Set OPENPAPER_LANG=<code> (ko, ja, zh, es, fr, de, pt; default ko) to pick the target
//! language for translate/job/import/redo/summary/askpaper/ask.

use openpaper::lang::Lang;
use openpaper::model::{Block, Builder, Doc};
use openpaper::translate::Mode;
use std::sync::atomic::AtomicBool;

fn load(arg: &str) -> Doc {
    if arg.ends_with(".pdf") {
        let bytes = std::fs::read(arg).expect("read pdf");
        let b: Builder = openpaper::pdf::parse(&bytes).expect("parse pdf");
        b.finish(format!("pdf-{}", openpaper::model::content_id(&bytes)), arg)
    } else {
        let id = openpaper::arxiv::parse_id(arg).expect("arxiv id");
        openpaper::arxiv::fetch(&id).expect("fetch")
    }
}

fn mode() -> Mode {
    if std::env::var("OPENPAPER_MODE").as_deref() == Ok("easy") {
        Mode::Easy
    } else {
        Mode::Plain
    }
}

fn lang() -> Lang {
    match std::env::var("OPENPAPER_LANG") {
        Ok(code) => Lang::from_code(&code).unwrap_or_else(|| {
            eprintln!("unknown OPENPAPER_LANG {code:?}; use one of ko ja zh es fr de pt");
            std::process::exit(2)
        }),
        Err(_) => Lang::Ko,
    }
}

/// The installed app's real library.
fn app_library() -> std::path::PathBuf {
    std::path::Path::new(&std::env::var_os("HOME").expect("HOME"))
        .join("Library/Application Support/dev.rlaope.openpaper/papers")
}

const USAGE: &str = "usage: openpaper-probe <parse|dump|translate|import|job|search|ask|claude|update|selfupdate|askpaper|summary|redo> <arg> ... (see the header of src/bin/probe.rs)";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || (args[1] != "update" && args.len() < 3) {
        eprintln!("{USAGE}");
        std::process::exit(2);
    }
    if args[1] == "update" {
        println!("{:?}", openpaper::update::check());
        return;
    }
    if args[1] == "selfupdate" {
        // Runs the in-app updater as if the app lived at <exe>, e.g. a test copy's
        // `<dir>/OpenPaper.app/Contents/MacOS/openpaper`.
        let log = std::env::temp_dir().join(openpaper::update::LOG_NAME);
        println!(
            "{:?} log={}",
            openpaper::update::install_latest(std::path::Path::new(&args[2]), &log),
            log.display()
        );
        return;
    }
    let (cmd, target) = (args[1].as_str(), args[2].as_str());
    if cmd == "askpaper" {
        // Uses the installed app's real library.
        let state = std::sync::Arc::new(openpaper::AppState::new(app_library()).expect("state"));
        let started = std::time::Instant::now();
        let mut first: Option<f32> = None;
        let mut pieces = 0usize;
        let a = openpaper::commands::ask_paper_stream(
            &state,
            &args[3],
            args.get(4).map(String::as_str),
            Some(target),
            &[],
            lang(),
            |d| {
                use std::io::Write;
                first.get_or_insert(started.elapsed().as_secs_f32());
                pieces += 1;
                eprint!("{d}");
                let _ = std::io::stderr().flush();
            },
        )
        .expect("ask");
        eprintln!();
        println!(
            "first {:.1}s, done {:.1}s, {pieces} pieces\n{}",
            first.unwrap_or_default(),
            started.elapsed().as_secs_f32(),
            a.reply
        );
        return;
    }
    if cmd == "summary" {
        // A fresh summary, saved into <dir> (a copy of the library) or the app's real library.
        let dir = match args.get(3) {
            Some(d) => std::path::PathBuf::from(d),
            None => app_library(),
        };
        let state = std::sync::Arc::new(openpaper::AppState::new(dir).expect("state"));
        let started = std::time::Instant::now();
        let mut first: Option<f32> = None;
        let r = openpaper::commands::summarize(&state, target, true, lang(), |_| {
            first.get_or_insert(started.elapsed().as_secs_f32());
        })
        .expect("summary");
        println!(
            "first {:.1}s, done {:.1}s, {} chars\n{}",
            first.unwrap_or_default(),
            started.elapsed().as_secs_f32(),
            r.reply.chars().count(),
            r.reply
        );
        return;
    }
    if cmd == "redo" {
        // Retranslates a saved paper in the current style, like the app's Retranslate button.
        let dir = std::path::PathBuf::from(&args[3]);
        let state = std::sync::Arc::new(openpaper::AppState::new(dir).expect("state"));
        let before = openpaper::commands::status(&state, target, Mode::Plain, lang());
        println!("outdated before {}", before.outdated);
        let started = std::time::Instant::now();
        openpaper::commands::start_translation(
            &state,
            target.to_string(),
            Mode::Plain,
            lang(),
            true,
        )
        .expect("start");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3));
            let s = openpaper::commands::status(&state, target, Mode::Plain, lang());
            println!(
                "{:>4.0}s units {}/{} redo {} running {} error {:?}",
                started.elapsed().as_secs_f32(),
                s.done,
                s.total,
                s.redo,
                s.running,
                s.error
            );
            if !s.running {
                println!("outdated after {}", s.outdated);
                return;
            }
        }
    }
    if cmd == "claude" {
        println!("{:?}", openpaper::translate::claude_status());
        return;
    }
    if cmd == "ask" {
        let started = std::time::Instant::now();
        let a = openpaper::recommend::recommend(target, &[], lang()).expect("recommend");
        println!("{:.1}s  {}", started.elapsed().as_secs_f32(), a.reply);
        for p in a.picks {
            println!(
                "{:<16} {} {} — {}",
                p.hit.id, p.hit.published, p.hit.title, p.why
            );
        }
        return;
    }
    if cmd == "search" {
        let start = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
        let p = openpaper::search::search(target, start).expect("search");
        println!("total {} start {}", p.total, p.start);
        for h in p.hits {
            println!(
                "{:<16} {} {:<8} {} — {}",
                h.id,
                h.published,
                h.category,
                h.title,
                h.authors.join(", ")
            );
        }
        return;
    }
    let doc = load(target);
    let mut counts = std::collections::BTreeMap::new();
    let mut embedded = 0;
    for b in &doc.blocks {
        let k = match b {
            Block::Heading { .. } => "heading",
            Block::Para { .. } => "para",
            Block::Static { .. } => "static",
            Block::Figure { images, .. } => {
                embedded += images.iter().filter(|i| i.starts_with("data:")).count();
                "figure"
            }
            Block::Equation { .. } => "equation",
            Block::Table { .. } => "table",
        };
        *counts.entry(k).or_insert(0) += 1;
    }
    let units = doc.units();
    let sentences: usize = units.iter().map(|u| u.sentences.len()).sum();
    println!("title: {}", doc.title);
    println!("source: {}", doc.source);
    println!("blocks: {counts:?}");
    println!(
        "units: {} sentences: {sentences} math: {} embedded images: {embedded}",
        units.len(),
        doc.math.len()
    );
    for b in doc
        .blocks
        .iter()
        .filter(|b| matches!(b, Block::Heading { .. }))
        .take(40)
    {
        if let Block::Heading { level, unit } = b {
            println!(
                "  {}{}",
                "  ".repeat(*level as usize - 1),
                unit.sentences.join(" ")
            );
        }
    }
    if cmd == "dump" {
        for b in &doc.blocks {
            let line = match b {
                Block::Heading { level, unit } => {
                    format!("H{level} {}", unit.sentences.join(" | "))
                }
                Block::Para { unit } => format!("P  {}", unit.sentences.join(" | ")),
                Block::Static { text, code } => {
                    format!("S{} {text}", if *code { "c" } else { " " })
                }
                Block::Figure { images, caption } => format!(
                    "F  {} imgs; {:?}",
                    images.len(),
                    caption.as_ref().map(|c| &c.sentences)
                ),
                Block::Equation { tex } => format!("E  {}", tex.join(" \\\\ ")),
                Block::Table { caption, rows } => format!(
                    "T  {}x{}; {:?}",
                    rows.len(),
                    rows.first().map_or(0, |r| r.len()),
                    caption.as_ref().map(|c| &c.sentences)
                ),
            };
            println!("{}", line.chars().take(220).collect::<String>());
        }
    }
    if cmd == "job" {
        // Runs the same background job the app starts, on a library dir.
        let dir = std::path::PathBuf::from(&args[3]);
        let state = std::sync::Arc::new(openpaper::AppState::new(dir).expect("state"));
        state.library.save(&doc).expect("save");
        let started = std::time::Instant::now();
        openpaper::commands::start_translation(&state, doc.id.clone(), mode(), lang(), false)
            .expect("start");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3));
            let s = openpaper::commands::status(&state, &doc.id, mode(), lang());
            println!(
                "{:>4.0}s batches {}/{} units {} running {} error {:?}",
                started.elapsed().as_secs_f32(),
                s.done,
                s.total,
                s.units.len(),
                s.running,
                s.error
            );
            if !s.running {
                let bad = doc
                    .units()
                    .iter()
                    .filter(|u| {
                        s.units
                            .get(&u.id)
                            .is_none_or(|k| k.len() != u.sentences.len())
                    })
                    .count();
                println!(
                    "units {} translated {} misaligned-or-missing {bad}",
                    doc.units().len(),
                    s.units.len()
                );
                return;
            }
        }
    }
    if cmd == "import" {
        let dir = std::path::PathBuf::from(&args[3]);
        let n: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(20);
        let lib = openpaper::library::Library::open(dir).expect("library");
        lib.save(&doc).expect("save");
        let claude = openpaper::translate::find_claude().expect("claude cli");
        let pending: Vec<_> = units.into_iter().take(n).cloned().collect();
        for batch in openpaper::translate::batches(&pending) {
            let out = openpaper::translate::translate_batch(
                &claude,
                &doc.title,
                &batch,
                &doc.heading_ids(),
                mode(),
                lang(),
                &AtomicBool::new(false),
            )
            .expect("translate");
            lib.add_translations(&doc.id, mode(), lang(), &out)
                .expect("save translations");
        }
        println!(
            "saved {} with {} translated units",
            doc.id,
            lib.translations(&doc.id, mode(), lang()).len()
        );
        return;
    }
    if cmd == "translate" {
        let n: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(6);
        let claude = openpaper::translate::find_claude().expect("claude cli");
        let batch: Vec<_> = units.into_iter().take(n).cloned().collect();
        let started = std::time::Instant::now();
        let out = openpaper::translate::translate_batch(
            &claude,
            &doc.title,
            &batch,
            &doc.heading_ids(),
            mode(),
            lang(),
            &AtomicBool::new(false),
        )
        .expect("translate");
        println!("translated {} units in {:?}", out.len(), started.elapsed());
        let tag = lang().code().to_uppercase();
        for (en, tr) in batch.iter().zip(&out) {
            for (e, t) in en.sentences.iter().zip(&tr.sentences) {
                println!("EN {e}\n{tag} {t}\n");
            }
        }
    }
}
