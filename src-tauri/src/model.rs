use crate::sentence;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A translatable piece of text, already split into sentences.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Unit {
    pub id: u32,
    pub sentences: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Block {
    Heading {
        level: u8,
        unit: Unit,
    },
    Para {
        unit: Unit,
    },
    /// Shown once across both columns and never translated (references, code, bare math).
    Static {
        text: String,
        code: bool,
    },
    Figure {
        images: Vec<String>,
        caption: Option<Unit>,
    },
    /// One TeX row per line; equation numbers are kept as `\tag{..}`.
    Equation {
        tex: Vec<String>,
    },
    Table {
        caption: Option<Unit>,
        rows: Vec<Vec<String>>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Doc {
    pub id: String,
    pub title: String,
    pub source: String,
    pub blocks: Vec<Block>,
    /// TeX sources referenced from text as `⟦M<index>⟧`.
    pub math: Vec<String>,
}

impl Doc {
    /// Units that get translated: section headings, paragraphs and captions. The paper title
    /// (level-1 heading) stays in English, shown once across both columns.
    pub fn units(&self) -> Vec<&Unit> {
        self.blocks
            .iter()
            .filter_map(|b| match b {
                Block::Heading { level, unit } if *level > 1 => Some(unit),
                Block::Para { unit } => Some(unit),
                Block::Figure { caption, .. } | Block::Table { caption, .. } => caption.as_ref(),
                _ => None,
            })
            .collect()
    }
}

impl Doc {
    /// Ids of heading units, so the translator can render them as titles rather than sentences.
    pub fn heading_ids(&self) -> std::collections::HashSet<u32> {
        self.blocks
            .iter()
            .filter_map(|b| match b {
                Block::Heading { unit, .. } => Some(unit.id),
                _ => None,
            })
            .collect()
    }
}

pub fn content_id(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .take(10)
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn normalize(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    // Removed footnote markers leave "word ." behind.
    let mut out = String::with_capacity(joined.len());
    let mut chars = joined.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ' ' && matches!(chars.peek(), Some('.' | ',' | ';' | ':' | ')')) {
            continue;
        }
        out.push(c);
    }
    out
}

/// True when the text has letters outside math placeholders.
fn has_prose(text: &str) -> bool {
    let mut in_token = false;
    for c in text.chars() {
        match c {
            '⟦' => in_token = true,
            '⟧' => in_token = false,
            c if !in_token && c.is_alphabetic() => return true,
            _ => {}
        }
    }
    false
}

#[derive(Default)]
pub struct Builder {
    pub title: String,
    pub blocks: Vec<Block>,
    pub math: Vec<String>,
    /// List marker waiting to be attached to the next paragraph.
    pub prefix: Option<String>,
    next: u32,
}

impl Builder {
    pub fn math(&mut self, tex: &str) -> String {
        self.math.push(normalize(tex));
        format!("⟦M{}⟧", self.math.len() - 1)
    }

    pub fn unit(&mut self, text: &str) -> Option<Unit> {
        let text = normalize(text);
        if !has_prose(&text) {
            return None;
        }
        let id = self.next;
        self.next += 1;
        Some(Unit {
            id,
            sentences: sentence::split(&text),
        })
    }

    pub fn heading(&mut self, level: u8, text: &str) {
        if let Some(unit) = self.unit(text) {
            self.blocks.push(Block::Heading { level, unit });
        }
    }

    pub fn para(&mut self, text: &str) {
        let text = match self.prefix.take() {
            Some(p) => format!("{p} {text}"),
            None => text.to_string(),
        };
        match self.unit(&text) {
            Some(unit) => self.blocks.push(Block::Para { unit }),
            None => self.fixed(&text, false),
        }
    }

    pub fn fixed(&mut self, text: &str, code: bool) {
        let text = if code {
            text.trim_matches('\n').to_string()
        } else {
            normalize(text)
        };
        if !text.trim().is_empty() {
            self.blocks.push(Block::Static { text, code });
        }
    }

    pub fn translatable_count(&self) -> u32 {
        self.next
    }

    pub fn finish(self, id: String, source: &str) -> Doc {
        Doc {
            id,
            title: self.title,
            source: source.to_string(),
            blocks: self.blocks,
            math: self.math,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_drops_space_before_punctuation() {
        assert_eq!(
            normalize("small  gradients .\n Next ( a ) , b"),
            "small gradients. Next ( a), b"
        );
    }

    #[test]
    fn math_only_text_is_not_translated() {
        let mut b = Builder::default();
        let token = b.math("x^2");
        b.para(&token);
        b.para("Hello world.");
        assert!(matches!(b.blocks[0], Block::Static { .. }));
        assert!(matches!(b.blocks[1], Block::Para { .. }));
        assert_eq!(b.translatable_count(), 1);
    }

    #[test]
    fn list_prefix_attaches_to_next_paragraph() {
        let mut b = Builder {
            prefix: Some("•".into()),
            ..Default::default()
        };
        b.para("First item.");
        b.para("Plain.");
        let Block::Para { unit } = &b.blocks[0] else {
            panic!()
        };
        assert_eq!(unit.sentences, vec!["• First item."]);
        let Block::Para { unit } = &b.blocks[1] else {
            panic!()
        };
        assert_eq!(unit.sentences, vec!["Plain."]);
    }

    #[test]
    fn headings_are_translated_and_marked() {
        let mut b = Builder::default();
        b.heading(1, "Sparse Memory");
        b.heading(2, "Abstract");
        b.para("We study memory.");
        let doc = b.finish("x".into(), "arxiv");
        let units = doc.units();
        assert_eq!(units.len(), 2, "the paper title stays in English");
        assert_eq!(units[0].sentences, vec!["Abstract"]);
        assert!(doc.heading_ids().contains(&units[0].id));
    }
}
