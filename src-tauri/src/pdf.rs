//! Best-effort structure recovery from PDF text.

use crate::model::{Builder, normalize};
use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

static NUMBERED_HEADING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:\d{1,2}(?:\.\d{1,2}){0,3}\.?|[A-H](?:\.\d{1,2}){0,2}|[IVX]{1,5}\.)\s+[A-Z][^.!?]*$",
    )
    .unwrap()
});
static PAGE_NOISE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:\d{1,4}|page \d+( of \d+)?|arXiv:\S+.*)$").unwrap());

const NAMED_HEADINGS: &[&str] = &[
    "abstract",
    "introduction",
    "background",
    "related work",
    "method",
    "methods",
    "results",
    "discussion",
    "conclusion",
    "conclusions",
    "acknowledgments",
    "acknowledgements",
    "references",
    "bibliography",
    "appendix",
];

fn is_reference_heading(s: &str) -> bool {
    matches!(s.to_lowercase().as_str(), "references" | "bibliography")
}

fn heading_level(line: &str) -> Option<u8> {
    let s = line.trim();
    if s.len() > 90 || s.is_empty() {
        return None;
    }
    let bare = s.trim_end_matches(':').to_lowercase();
    if NAMED_HEADINGS.contains(&bare.as_str()) {
        return Some(2);
    }
    if NUMBERED_HEADING.is_match(s) && s.split_whitespace().count() <= 12 {
        let number = s.split_whitespace().next().unwrap_or("");
        let depth = number.trim_end_matches('.').matches('.').count() as u8;
        return Some((2 + depth).min(4));
    }
    None
}

/// Lowercase words that occur unbroken inside a line, used to tell
/// syllable hyphens ("multi-|plicative") from compound hyphens ("sequence-|aligned").
fn vocabulary(text: &str) -> HashSet<String> {
    let mut words = HashSet::new();
    let mut prev_hyphenated = false;
    for line in text.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        // The last token may be the head of a line-broken word, and the first
        // token the tail of one when the previous line ended with '-'.
        let skip = usize::from(prev_hyphenated);
        prev_hyphenated = tokens.last().is_some_and(|t| t.ends_with('-'));
        for token in tokens
            .iter()
            .take(tokens.len().saturating_sub(1))
            .skip(skip)
        {
            let word = token.trim_matches(|c: char| !c.is_alphabetic());
            if word.len() >= 3 && word.chars().all(|c| c.is_alphabetic()) {
                words.insert(word.to_lowercase());
            }
        }
    }
    words
}

fn alpha_prefix(s: &str) -> String {
    s.chars()
        .take_while(|c| c.is_alphabetic())
        .collect::<String>()
        .to_lowercase()
}

/// Joins wrapped lines, removing end-of-line hyphenation before a lowercase continuation.
fn join_lines(lines: &[&str], vocab: &HashSet<String>) -> String {
    let mut out = String::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let hyphenated = out.ends_with('-') && !out.ends_with("--");
        let lower_next = line.chars().next().is_some_and(|c| c.is_lowercase());
        let next_word = line.split_whitespace().next().unwrap_or("");
        let compound = hyphenated && {
            // `out` ends with the one-byte '-', so this slice is on a char boundary.
            let left = out[..out.len() - 1]
                .rsplit(|c: char| !c.is_alphabetic())
                .next()
                .unwrap_or("")
                .to_lowercase();
            let right = alpha_prefix(next_word);
            // A whole word on the left whose join with the right never occurs
            // elsewhere ("sequence-|aligned") is a compound, not a split syllable.
            left.len() >= 4
                && right.len() >= 4
                && vocab.contains(&left)
                && !vocab.contains(&format!("{left}{right}"))
        };
        if hyphenated && lower_next && (next_word.contains('-') || compound) {
            // "English-" + "to-German", "sequence-" + "aligned": the hyphen belongs to the word.
        } else if hyphenated && lower_next {
            out.pop();
        } else if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(line);
    }
    normalize(&out)
}

/// Short, numeric, or symbol-heavy lines usually come from tables and diagrams.
fn looks_tabular(lines: &[&str]) -> bool {
    if lines.len() < 3 {
        return false;
    }
    let avg = lines
        .iter()
        .map(|l| l.trim().chars().count())
        .sum::<usize>()
        / lines.len();
    let text: String = lines.concat();
    let letters = text.chars().filter(|c| c.is_alphabetic()).count();
    let total = text.chars().filter(|c| !c.is_whitespace()).count().max(1);
    avg < 28 || letters * 100 / total < 55
}

fn looks_like_title(s: &str) -> bool {
    let words = s.split_whitespace().count();
    (2..=25).contains(&words) && s.len() <= 200 && !s.contains('@') && !s.ends_with('.')
}

pub fn structure(text: &str) -> Builder {
    let mut b = Builder::default();
    let text = text.replace('\u{c}', "\n\n").replace("\r\n", "\n");
    let vocab = vocabulary(&text);
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                blocks.push(std::mem::take(&mut current));
            }
        } else if !PAGE_NOISE.is_match(line.trim()) {
            // A heading can follow the previous paragraph without a blank line.
            let after_sentence = current.last().is_some_and(|l| {
                l.trim_end().ends_with(['.', ':', ')', ']']) || l.contains("arXiv:")
            });
            if after_sentence && heading_level(line).is_some() {
                blocks.push(std::mem::take(&mut current));
            }
            current.push(line);
        }
    }
    if !current.is_empty() {
        blocks.push(current);
    }

    let has_abstract = blocks.iter().any(|bl| {
        bl[0]
            .trim()
            .trim_end_matches(':')
            .eq_ignore_ascii_case("abstract")
    });
    let mut in_front = has_abstract;
    let mut in_refs = false;
    for lines in blocks {
        // A heading often sits on the first line of a block followed by body text.
        let first = lines[0].trim();
        let mut body = &lines[..];
        let is_abstract = first.trim_end_matches(':').eq_ignore_ascii_case("abstract");
        // Front matter (title, authors, affiliations) has no section headings before the abstract.
        let level = if in_front && !is_abstract {
            None
        } else {
            heading_level(first)
        };
        if let Some(level) = level {
            in_front = false;
            b.heading(level, first);
            in_refs = is_reference_heading(first) || (in_refs && level > 2);
            body = &lines[1..];
            if body.is_empty() {
                continue;
            }
        }
        let joined = join_lines(body, &vocab);
        if in_front {
            if b.title.is_empty() && looks_like_title(&joined) {
                b.title = joined.clone();
                b.heading(1, &joined);
            } else {
                b.fixed(&joined, false);
            }
        } else if in_refs || looks_tabular(body) {
            b.fixed(&joined, false);
        } else {
            b.para(&joined);
        }
    }
    b
}

pub fn parse(bytes: &[u8]) -> Result<Builder, String> {
    let owned = bytes.to_vec();
    let text = std::panic::catch_unwind(move || pdf_extract::extract_text_from_mem(&owned))
        .map_err(|_| "Can't read this PDF".to_string())?
        .map_err(|_| "Can't read this PDF (it may be encrypted or damaged)".to_string())?;
    let b = structure(&text);
    if b.translatable_count() == 0 {
        return Err("No text found in the PDF (it may be a scan)".into());
    }
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Block;

    const SAMPLE: &str = "Provided permission text.\n\nA Great Paper Title\n\nJane Doe\njane@x.org\n\nAbstract\n\nWe propose a new and simple training\nmethod. It is both multi-\nplicative and remarkably fast.\n\n1 Introduction\n\nDeep learning is popular on large clusters with\n8 GPUs. It works well for most of the tasks that\nwe evaluated in this paper.\n3.1 Setup Details\nWe train on data.\n\nReferences\n\n[1] A. Author. Some book. 2020.\n";

    #[test]
    fn recovers_headings_front_matter_and_references() {
        let b = structure(SAMPLE);
        assert_eq!(b.title, "A Great Paper Title");
        let view: Vec<String> = b
            .blocks
            .iter()
            .map(|b| match b {
                Block::Heading { level, unit } => format!("h{level}:{}", unit.sentences.join("|")),
                Block::Para { unit } => format!("p:{}", unit.sentences.join("|")),
                Block::Static { text, .. } => format!("s:{text}"),
                _ => "?".into(),
            })
            .collect();
        assert_eq!(
            view,
            vec![
                "s:Provided permission text.",
                "h1:A Great Paper Title",
                "s:Jane Doe jane@x.org",
                "h2:Abstract",
                "p:We propose a new and simple training method.|It is both multiplicative and remarkably fast.",
                "h2:1 Introduction",
                "p:Deep learning is popular on large clusters with 8 GPUs.|It works well for most of the tasks that we evaluated in this paper.",
                "h3:3.1 Setup Details",
                "p:We train on data.",
                "h2:References",
                "s:[1] A. Author. Some book. 2020.",
            ]
        );
    }

    #[test]
    fn keeps_compound_hyphens() {
        let none = HashSet::new();
        assert_eq!(
            join_lines(&["English-", "to-German"], &none),
            "English-to-German"
        );
        assert_eq!(
            join_lines(&["multi-", "plicative"], &none),
            "multiplicative"
        );
        let vocab = vocabulary(
            "the sequence of tokens and\naligned with each other\nthe se-\nquence again\n",
        );
        assert!(!vocab.contains("quence"));
        assert_eq!(
            join_lines(&["of sequence-", "aligned RNNs"], &vocab),
            "of sequence-aligned RNNs"
        );
        assert_eq!(join_lines(&["the se-", "quence"], &vocab), "the sequence");
        assert_eq!(
            join_lines(&["Ashish Vaswani∗", "Google"], &vocab),
            "Ashish Vaswani∗ Google"
        );
        assert_eq!(
            join_lines(&["WMT English-", "German"], &none),
            "WMT English- German"
        );
    }
}
