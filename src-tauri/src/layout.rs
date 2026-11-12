//! Reads where each glyph of a PDF sits and rebuilds lines in reading order.
//!
//! Plain text extraction loses the page geometry, which breaks review preprints: the line
//! numbers in both margins end up inside sentences, the two columns run together, tables
//! become strings of numbers and subscripts sit on the baseline (`R0`, `TopKK0,pd`). This
//! module keeps the positions, so it can:
//! - drop margin line numbers (a column of short numbers outside the text body),
//! - read a two-column page column by column,
//! - write sub- and superscripts as TeX (`R_{0}`) for the reader's KaTeX,
//! - turn aligned rows into a table, a line ending in `(n)` into a display equation and
//!   numbered pseudocode into a code block.
//!
//! The result is a list of [`Item`]s that `pdf::assemble` turns into blocks.

use crate::model::{Block, Builder};
use pdf_extract::{MediaBox, OutputDev, OutputError, Transform};
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    pub x: f64,
    /// Baseline, measured down from the top of the page.
    pub y: f64,
    pub end: f64,
    pub size: f64,
    pub text: String,
    /// A combining accent drawn over this glyph (`ˆ` over `a`).
    pub accent: Option<char>,
    /// Position in the page's content stream.
    pub seq: usize,
    /// The PDF drew a space right after this glyph.
    pub space_after: bool,
}

impl Glyph {
    #[cfg(test)]
    pub fn new(x: f64, y: f64, size: f64, text: &str) -> Self {
        let width = 0.5 * size * text.chars().count() as f64;
        Glyph {
            x,
            y,
            end: x + width,
            size,
            text: text.to_string(),
            accent: None,
            seq: 0,
            space_after: false,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Item {
    /// The paper's title: the largest text on the first page.
    Title(String),
    Line(String),
    /// A paragraph break.
    Break,
    Block(Block),
}

/// One page: its size and every glyph on it.
#[derive(Clone, Debug, Default)]
pub struct Page {
    pub width: f64,
    pub height: f64,
    pub glyphs: Vec<Glyph>,
}

struct Collector {
    pages: Vec<Page>,
    flip: Transform,
}

impl OutputDev for Collector {
    fn begin_page(
        &mut self,
        _page: u32,
        media_box: &MediaBox,
        _art_box: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), OutputError> {
        let height = media_box.ury - media_box.lly;
        self.flip = Transform::row_major(1., 0., 0., -1., 0., height);
        self.pages.push(Page {
            width: media_box.urx - media_box.llx,
            height,
            glyphs: Vec::new(),
        });
        Ok(())
    }
    fn end_page(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn output_character(
        &mut self,
        trm: &Transform,
        width: f64,
        _spacing: f64,
        font_size: f64,
        text: &str,
    ) -> Result<(), OutputError> {
        // Rotated or mirrored text (margin stamps, rotated headers) is not part of the reading flow.
        let tilt = trm.m12.abs().max(trm.m21.abs());
        if trm.m11 <= 0. || trm.m22 <= 0. || tilt > 0.01 * trm.m11.min(trm.m22) {
            return Ok(());
        }
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        if text.trim().is_empty() {
            // A space glyph marks a word break even where the gap is tight.
            if let Some(g) = self.pages.last_mut().and_then(|p| p.glyphs.last_mut()) {
                g.space_after = true;
            }
            return Ok(());
        }
        let size = font_size * (trm.m11 * trm.m22).sqrt();
        if !(1.0..200.0).contains(&size) {
            return Ok(());
        }
        let p = trm.post_transform(&self.flip);
        let x = p.m31;
        if let Some(page) = self.pages.last_mut() {
            let seq = page.glyphs.len();
            page.glyphs.push(Glyph {
                x,
                y: p.m32,
                end: x + width * size,
                size,
                text,
                accent: None,
                seq,
                space_after: false,
            });
        }
        Ok(())
    }
    fn begin_word(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn end_word(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
    fn end_line(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
}

/// Every page's glyphs with their positions.
pub fn glyphs(bytes: &[u8]) -> Result<Vec<Page>, String> {
    let mut doc = pdf_extract::Document::load_mem(bytes)
        .map_err(|_| "Can't read this PDF (it may be encrypted or damaged)".to_string())?;
    if doc.is_encrypted() {
        doc.decrypt("")
            .map_err(|_| "Can't read this PDF (it may be encrypted or damaged)".to_string())?;
    }
    let mut c = Collector {
        pages: Vec::new(),
        flip: Transform::identity(),
    };
    pdf_extract::output_doc(&doc, &mut c).map_err(|_| "Can't read this PDF".to_string())?;
    Ok(c.pages)
}

/// Glyphs that share a baseline, left to right.
#[derive(Clone, Debug)]
struct Line {
    y: f64,
    size: f64,
    glyphs: Vec<Glyph>,
}

impl Line {
    fn new(glyphs: Vec<Glyph>) -> Self {
        // The base size is the largest size that holds a third of the glyphs: scripts can
        // outnumber the base letters (`A_{core}`) but are never larger. The baseline is where
        // the base-size glyphs sit.
        let mut sizes: HashMap<i64, usize> = HashMap::new();
        for g in &glyphs {
            *sizes.entry((g.size * 10.).round() as i64).or_default() += 1;
        }
        let common = sizes
            .iter()
            .max_by_key(|(k, n)| (**n, **k))
            .map_or(100, |(k, _)| *k);
        let key = sizes
            .iter()
            .filter(|(_, n)| **n * 3 >= glyphs.len())
            .map(|(k, _)| *k)
            .max()
            .unwrap_or(common);
        let mut ys: Vec<f64> = glyphs
            .iter()
            .filter(|g| (g.size * 10.).round() as i64 == key)
            .map(|g| g.y)
            .collect();
        ys.sort_by(f64::total_cmp);
        Line {
            y: ys.get(ys.len() / 2).copied().unwrap_or(0.),
            size: key as f64 / 10.,
            glyphs,
        }
    }
    fn x0(&self) -> f64 {
        self.glyphs.iter().map(|g| g.x).fold(f64::MAX, f64::min)
    }
    fn x1(&self) -> f64 {
        self.glyphs.iter().map(|g| g.end).fold(0., f64::max)
    }
    /// Index ranges of glyph groups separated by wide gaps (table cells, margin numbers).
    fn runs(&self) -> Vec<std::ops::Range<usize>> {
        let mut out = Vec::new();
        let mut start = 0;
        for i in 1..self.glyphs.len() {
            let reach = self.glyphs[..i].iter().map(|g| g.end).fold(0., f64::max);
            if self.glyphs[i].x - reach > 0.8 * self.size {
                out.push(start..i);
                start = i;
            }
        }
        if !self.glyphs.is_empty() {
            out.push(start..self.glyphs.len());
        }
        out
    }
    fn plain(&self, r: std::ops::Range<usize>) -> String {
        words(&self.glyphs[r], self.size)
            .iter()
            .map(|w| w.iter().map(|g| g.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Groups glyphs into lines by baseline. Sub- and superscripts stay with their line.
fn lines(mut glyphs: Vec<Glyph>) -> Vec<Line> {
    glyphs.sort_by(|a, b| a.y.total_cmp(&b.y));
    let mut groups: Vec<(f64, f64, Vec<Glyph>)> = Vec::new();
    for g in glyphs {
        if let Some((y, size, members)) = groups.last_mut() {
            if (g.y - *y).abs() <= 0.45 * size.max(g.size) {
                if g.size > *size + 0.01 {
                    *size = g.size;
                    *y = g.y;
                }
                members.push(g);
                continue;
            }
        }
        groups.push((g.y, g.size, vec![g]));
    }
    let mut out: Vec<Line> = groups
        .into_iter()
        .flat_map(|(_, _, members)| untangle(members))
        .map(Line::new)
        .collect();
    merge_scripts(&mut out);
    for l in &mut out {
        l.glyphs.sort_by(|a, b| a.x.total_cmp(&b.x));
        attach_accents(&mut l.glyphs);
    }
    out
}

/// Splits a baseline group whose glyphs overlap: labels of a figure drawn across body text
/// share its baseline, and sorting both by x would interleave their letters. Glyphs are cut
/// into the pieces the PDF drew in one go (a jump back to the left starts a new piece); the
/// biggest pieces that don't overlap each other stay together, the rest form lines of their own.
fn untangle(mut glyphs: Vec<Glyph>) -> Vec<Vec<Glyph>> {
    glyphs.sort_by_key(|g| g.seq);
    let mut pieces: Vec<Vec<Glyph>> = Vec::new();
    for g in glyphs {
        match pieces.last_mut() {
            Some(p)
                if p.last()
                    .is_some_and(|l| g.x >= l.x - 0.3 * l.size && g.seq == l.seq + 1) =>
            {
                p.push(g)
            }
            _ => pieces.push(vec![g]),
        }
    }
    if pieces.len() == 1 {
        return pieces;
    }
    let span = |p: &[Glyph]| {
        (
            p.iter().map(|g| g.x).fold(f64::MAX, f64::min),
            p.iter().map(|g| g.end).fold(f64::MIN, f64::max),
        )
    };
    let overlaps = |a: &[Glyph], b: &[Glyph]| {
        a.iter().any(|x| {
            b.iter().any(|y| {
                let o = x.end.min(y.end) - x.x.max(y.x);
                o > 0.3 * x.size.min(y.size)
            })
        })
    };
    pieces.sort_by_key(|p| std::cmp::Reverse(p.len()));
    let size = |p: &[Glyph]| p.iter().map(|g| g.size).fold(0., f64::max);
    let base = |p: &[Glyph]| {
        let s = size(p);
        p.iter()
            .filter(|g| g.size == s)
            .map(|g| g.y)
            .next()
            .unwrap_or(0.)
    };
    let mut lines: Vec<Vec<Glyph>> = Vec::new();
    for p in pieces {
        // An accent always sits over a letter of some line.
        let accent = p.len() == 1 && accent_of(&p[0]).is_some();
        let (x0, x1) = span(&p);
        // Text of another size on the same baseline is a label, not a script.
        let other_size = |l: &[Glyph]| {
            let (a, b) = (size(l), size(&p));
            b > 1.1 * a || (a > 1.1 * b && (base(&p) - base(l)).abs() < 0.08 * a)
        };
        let home = lines.iter().position(|l| {
            accent && {
                let (a, b) = span(l);
                a <= x1 && x0 <= b
            } || (!overlaps(l, &p) && !other_size(l))
        });
        match home {
            Some(i) => lines[i].extend(p),
            None => lines.push(p),
        }
    }
    lines
}

/// Folds a line of only small glyphs into the larger line right above or below it, when it
/// sits within that line's span: scripts that fell out of their line (`X_B` drawn lower than
/// usual, limits under a big operator).
fn merge_scripts(lines: &mut Vec<Line>) {
    let mut i = 0;
    while i < lines.len() {
        let small = lines[i].glyphs.iter().map(|g| g.size).fold(0., f64::max);
        // A lone big operator (`⋃`, `∑`) is drawn off the baseline too.
        let symbols = lines[i].glyphs.len() <= 3
            && lines[i]
                .glyphs
                .iter()
                .all(|g| !g.text.chars().any(char::is_alphanumeric));
        let (x0, x1) = (lines[i].x0(), lines[i].x1());
        let fits = |j: usize, lines: &Vec<Line>| {
            let n = &lines[j];
            // Scripts sit beside the letters of their line, never on top of them (two margin
            // numbers of neighbouring lines would).
            let clash = lines[i].glyphs.iter().any(|a| {
                n.glyphs
                    .iter()
                    .any(|b| a.end.min(b.end) - a.x.max(b.x) > 0.3 * a.size.min(b.size))
            });
            (small < 0.85 * n.size || symbols)
                && !clash
                && n.glyphs.len() > lines[i].glyphs.len()
                && (n.y - lines[i].y).abs() < if symbols { 1.0 } else { 0.75 } * n.size.max(small)
                && x0 >= n.x0() - n.size
                && x1 <= n.x1() + n.size
        };
        let near = |j: usize| (lines[j].y - lines[i].y).abs();
        let below = (i + 1 < lines.len()).then_some(i + 1);
        let target = if symbols && below.is_some_and(|j| fits(j, lines)) {
            // A big operator's glyph hangs from the top of its line.
            below
        } else {
            [i.checked_sub(1), below]
                .into_iter()
                .flatten()
                .filter(|&j| fits(j, lines))
                .min_by(|&a, &b| near(a).total_cmp(&near(b)))
        };
        match target {
            Some(j) => {
                let moved = std::mem::take(&mut lines[i].glyphs);
                let mut glyphs = std::mem::take(&mut lines[j].glyphs);
                glyphs.extend(moved);
                lines[j] = Line::new(glyphs);
                lines.remove(i);
                i = i.saturating_sub(1);
            }
            None => i += 1,
        }
    }
}

const ACCENTS: &[(char, &str, char)] = &[
    ('ˆ', "hat", '\u{302}'),
    ('^', "hat", '\u{302}'),
    ('˜', "tilde", '\u{303}'),
    ('~', "tilde", '\u{303}'),
    ('¯', "bar", '\u{304}'),
    ('˙', "dot", '\u{307}'),
    ('¨', "ddot", '\u{308}'),
    ('´', "acute", '\u{301}'),
    ('`', "grave", '\u{300}'),
];

fn accent_of(g: &Glyph) -> Option<char> {
    let mut chars = g.text.chars();
    let c = chars.next()?;
    (chars.next().is_none() && ACCENTS.iter().any(|(a, ..)| *a == c)).then_some(c)
}

/// Folds an accent glyph drawn over its neighbour into that neighbour, and a slash drawn over
/// a relation into its negation (`∈` + `/` → `∉`).
fn attach_accents(glyphs: &mut Vec<Glyph>) {
    let mut k = 1;
    while k < glyphs.len() {
        let (a, b) = (&glyphs[k - 1], &glyphs[k]);
        let negated = match (a.text.as_str(), b.text.as_str()) {
            ("∈", "/") | ("/", "∈") => Some("∉"),
            ("=", "/") | ("/", "=") => Some("≠"),
            ("⊂", "/") | ("/", "⊂") => Some("⊄"),
            ("⊆", "/") | ("/", "⊆") => Some("⊈"),
            _ => None,
        };
        let overlap = a.end.min(b.end) - a.x.max(b.x);
        if let Some(n) = negated.filter(|_| overlap > 0.2 * a.size.min(b.size)) {
            let x = a.x.min(b.x);
            let end = a.end.max(b.end);
            glyphs[k - 1].text = n.to_string();
            glyphs[k - 1].x = x;
            glyphs[k - 1].end = end;
            glyphs.remove(k);
            continue;
        }
        k += 1;
    }
    let mut i = 0;
    while i < glyphs.len() {
        if let Some(a) = accent_of(&glyphs[i]) {
            let g = &glyphs[i];
            let slack = 0.05 * g.size;
            let target =
                if i > 0 && glyphs[i - 1].end > g.x + slack && glyphs[i - 1].accent.is_none() {
                    Some(i - 1)
                } else if i + 1 < glyphs.len()
                    && glyphs[i + 1].x < g.end - slack
                    && glyphs[i + 1].accent.is_none()
                {
                    Some(i + 1)
                } else {
                    None
                };
            if let Some(t) = target {
                glyphs[t].accent = Some(a);
                glyphs.remove(i);
                continue;
            }
        }
        i += 1;
    }
}

fn is_line_number(s: &str) -> bool {
    (1..=4).contains(&s.len()) && s.chars().all(|c| c.is_ascii_digit())
}

/// Drops the line numbers that review templates print in the margins: short numbers that line
/// up in a column outside every other piece of text on the page.
fn drop_line_numbers(lines: &mut Vec<Line>) {
    struct Cand {
        line: usize,
        range: std::ops::Range<usize>,
        left: bool,
        key: i64,
        x0: f64,
        x1: f64,
    }
    let mut cands = Vec::new();
    let mut body_min = f64::MAX;
    let mut body_max = f64::MIN;
    let mid = (lines.iter().map(Line::x0).fold(f64::MAX, f64::min)
        + lines.iter().map(Line::x1).fold(f64::MIN, f64::max))
        / 2.;
    for (i, l) in lines.iter().enumerate() {
        let runs = l.runs();
        for (k, r) in runs.iter().enumerate() {
            let text = l.plain(r.clone());
            let x = l.glyphs[r.start].x;
            // A number on a line of its own counts for the margin it sits in.
            let first = k == 0 && (runs.len() > 1 || x < mid);
            let last = k + 1 == runs.len() && (runs.len() > 1 || x >= mid);
            let x0 = l.glyphs[r.start].x;
            let x1 = l.glyphs[r.clone()].iter().map(|g| g.end).fold(0., f64::max);
            if is_line_number(&text) && (first || last) {
                cands.push(Cand {
                    line: i,
                    range: r.clone(),
                    left: first,
                    key: (if first { x0 } else { x1 } / 3.).round() as i64,
                    x0,
                    x1,
                });
            } else if text.chars().count() > 3 {
                // Short leftovers (a lone digit or mark) say nothing about where the body is.
                body_min = body_min.min(x0);
                body_max = body_max.max(x1);
            }
        }
    }
    let mut count: HashMap<(bool, i64), usize> = HashMap::new();
    for c in &cands {
        *count.entry((c.left, c.key)).or_default() += 1;
    }
    let mut drop: HashMap<usize, Vec<std::ops::Range<usize>>> = HashMap::new();
    for c in cands {
        let outside = if c.left {
            c.x1 <= body_min + 1.
        } else {
            c.x0 >= body_max - 1.
        };
        if count[&(c.left, c.key)] >= 5 && outside {
            drop.entry(c.line).or_default().push(c.range);
        }
    }
    for (i, ranges) in drop {
        let l = &mut lines[i];
        let keep: Vec<Glyph> = l
            .glyphs
            .iter()
            .enumerate()
            .filter(|(k, _)| !ranges.iter().any(|r| r.contains(k)))
            .map(|(_, g)| g.clone())
            .collect();
        *l = Line::new(keep);
    }
    lines.retain(|l| !l.glyphs.is_empty());
}

/// The x of the gap between two columns, when the page has two.
fn gutter(lines: &[Line], width: f64) -> Option<f64> {
    if lines.len() < 8 || width <= 0. {
        return None;
    }
    let (lo, hi) = ((width * 0.35) as usize, (width * 0.65) as usize);
    let mut cover = vec![0usize; hi.saturating_sub(lo) + 1];
    for l in lines {
        let mut hit = vec![false; cover.len()];
        for r in l.runs() {
            let x0 = l.glyphs[r.start].x;
            let x1 = l.glyphs[r].iter().map(|g| g.end).fold(0., f64::max);
            for (b, h) in hit.iter_mut().enumerate() {
                let x = (lo + b) as f64 + 0.5;
                if x0 <= x && x <= x1 {
                    *h = true;
                }
            }
        }
        for (c, h) in cover.iter_mut().zip(hit) {
            *c += usize::from(h);
        }
    }
    // The gutter is the widest run of positions crossed by the fewest lines (a full-width
    // title or figure may cross it).
    let min = cover.iter().copied().min().unwrap_or(0);
    if min > lines.len() / 4 {
        return None;
    }
    let limit = min + lines.len() / 40;
    let (mut best, mut cur) = ((0, 0), None::<usize>);
    for (b, c) in cover.iter().enumerate() {
        if *c <= limit {
            let s = *cur.get_or_insert(b);
            if b + 1 - s > best.1 - best.0 {
                best = (s, b + 1);
            }
        } else {
            cur = None;
        }
    }
    if best.1 - best.0 < 4 {
        return None;
    }
    let gx = lo as f64 + (best.0 + best.1) as f64 / 2.;
    let left = lines
        .iter()
        .filter(|l| l.x0() < gx && l.x1() < gx + 1.)
        .count()
        + lines
            .iter()
            .filter(|l| l.x0() < gx && l.glyphs.iter().any(|g| g.x > gx))
            .count();
    let right = lines.iter().filter(|l| l.x1() > gx).count();
    (left >= 3 && right >= 3).then_some(gx)
}

fn crosses(l: &Line, gx: f64) -> bool {
    l.runs().into_iter().any(|r| {
        let x0 = l.glyphs[r.start].x;
        let x1 = l.glyphs[r].iter().map(|g| g.end).fold(0., f64::max);
        x0 < gx - 2. && gx + 2. < x1
    })
}

/// Where a line sits on the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Region {
    Full,
    Left,
    Right,
}

/// The x of an empty band shared by consecutive lines of two prose pieces each (two figure
/// captions side by side on a one-column page).
fn local_gutter(lines: &[Line]) -> Option<f64> {
    let (mut lo, mut hi) = (f64::MIN, f64::MAX);
    for l in lines {
        let runs = l.runs();
        if runs.len() != 2 {
            return None;
        }
        let gap0 = l.glyphs[runs[0].clone()]
            .iter()
            .map(|g| g.end)
            .fold(0., f64::max);
        let gap1 = l.glyphs[runs[1].start].x;
        for r in runs {
            let text = l.plain(r);
            let letters = text.chars().filter(|c| c.is_alphabetic()).count();
            if text.chars().count() < 12 || letters * 2 < text.chars().count() {
                return None;
            }
        }
        lo = lo.max(gap0);
        hi = hi.min(gap1);
    }
    (hi - lo > 1.5 * lines[0].size).then_some((lo + hi) / 2.)
}

/// Splits side-by-side blocks inside a one-column region into left then right.
fn split_local(lines: Vec<Line>) -> Vec<(Region, Vec<Line>)> {
    let mut out: Vec<(Region, Vec<Line>)> = Vec::new();
    let mut i = 0;
    let push_full = |out: &mut Vec<(Region, Vec<Line>)>, l: Line| match out.last_mut() {
        Some((Region::Full, v)) => v.push(l),
        _ => out.push((Region::Full, vec![l])),
    };
    let mut lines = lines.into_iter().map(Some).collect::<Vec<_>>();
    while i < lines.len() {
        let mut j = i;
        while j < lines.len() && local_gutter(&[lines[j].clone().unwrap()]).is_some() {
            j += 1;
        }
        let run: Vec<Line> = lines[i..j].iter().map(|l| l.clone().unwrap()).collect();
        if j - i >= 3 {
            if let Some(gx) = local_gutter(&run) {
                let (mut a, mut b) = (Vec::new(), Vec::new());
                for l in run {
                    let (x, y): (Vec<Glyph>, Vec<Glyph>) =
                        l.glyphs.into_iter().partition(|g| g.x < gx);
                    a.push(Line::new(x));
                    b.push(Line::new(y));
                }
                out.push((Region::Left, a));
                out.push((Region::Right, b));
                i = j;
                continue;
            }
        }
        push_full(&mut out, lines[i].take().unwrap());
        i += 1;
    }
    out
}

/// A page's lines in reading order, grouped into runs that share a region.
fn reading_order(mut lines: Vec<Line>, width: f64) -> Vec<(Region, Vec<Line>)> {
    lines.sort_by(|a, b| a.y.total_cmp(&b.y));
    let Some(gx) = gutter(&lines, width) else {
        return split_local(lines);
    };
    let mut full: Vec<bool> = lines.iter().map(|l| crosses(l, gx)).collect();
    // Rows of a wide table between two full-width rows are full width too.
    let near = |i: usize, j: usize| (lines[i].y - lines[j].y).abs() <= 2.5 * lines[i].size;
    for i in 0..lines.len() {
        if full[i] {
            continue;
        }
        let above = (0..i).rev().take_while(|&j| near(i, j)).any(|j| full[j]);
        let below = (i + 1..lines.len())
            .take_while(|&j| near(i, j))
            .any(|j| full[j]);
        if above && below {
            full[i] = true;
        }
    }
    let mut out: Vec<(Region, Vec<Line>)> = Vec::new();
    let (mut left, mut right): (Vec<Glyph>, Vec<Glyph>) = (Vec::new(), Vec::new());
    // Each column's glyphs are grouped into lines again on their own, so a superscript in one
    // column never joins a line of the other.
    let flush =
        |out: &mut Vec<(Region, Vec<Line>)>, left: &mut Vec<Glyph>, right: &mut Vec<Glyph>| {
            if !left.is_empty() {
                out.push((Region::Left, self::lines(std::mem::take(left))));
            }
            if !right.is_empty() {
                out.push((Region::Right, self::lines(std::mem::take(right))));
            }
        };
    for (l, is_full) in lines.into_iter().zip(full) {
        if is_full {
            flush(&mut out, &mut left, &mut right);
            match out.last_mut() {
                Some((Region::Full, v)) => v.push(l),
                _ => out.push((Region::Full, vec![l])),
            }
            continue;
        }
        let (a, b): (Vec<Glyph>, Vec<Glyph>) = l.glyphs.into_iter().partition(|g| g.x < gx);
        left.extend(a);
        right.extend(b);
    }
    flush(&mut out, &mut left, &mut right);
    out
}

/// Splits glyphs into words at visible gaps. A word that starts as a sub- or superscript
/// stays with the word before it (`c` `i`→ one word).
fn words(glyphs: &[Glyph], size: f64) -> Vec<Vec<Glyph>> {
    let mut out: Vec<Vec<Glyph>> = Vec::new();
    let mut reach = f64::MIN;
    let mut spaced = false;
    for g in glyphs {
        let script = g.size < 0.82 * size;
        // Word gaps scale with the glyph's own size (a small bold caption in a body line).
        let em = g.size.min(size);
        let gap = g.x - reach > 0.1 * em || (spaced && g.x - reach > 0.02 * em);
        spaced = g.space_after;
        match out.last_mut() {
            Some(w) if !gap || script => w.push(g.clone()),
            _ => out.push(vec![g.clone()]),
        }
        reach = reach.max(g.end);
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Ev<'a> {
    Open(bool),
    Close,
    Char(&'a Glyph),
}

/// Walks a word glyph by glyph, opening a subscript (`true`) or superscript (`false`) where
/// the glyphs shrink and leave the baseline.
fn script_events(glyphs: &[Glyph], base_y: f64, base_size: f64) -> Vec<Ev<'_>> {
    let mut stack = vec![(base_y, base_size)];
    let mut out = Vec::new();
    for g in glyphs {
        loop {
            let (ty, ts) = *stack.last().unwrap();
            let shift = g.y - ty;
            if g.size < 0.82 * ts && shift.abs() > 0.06 * ts {
                stack.push((g.y, g.size));
                out.push(Ev::Open(shift > 0.));
                out.push(Ev::Char(g));
                break;
            }
            let same = (g.size - ts).abs() <= 0.12 * ts;
            if stack.len() > 1 && (g.size > 1.15 * ts || (same && shift.abs() > 0.25 * ts)) {
                stack.pop();
                out.push(Ev::Close);
                continue;
            }
            out.push(Ev::Char(g));
            break;
        }
    }
    for _ in 1..stack.len() {
        out.push(Ev::Close);
    }
    out
}

fn has_script(evs: &[Ev]) -> bool {
    evs.iter().any(|e| matches!(e, Ev::Open(_)))
}

fn tex_char(c: char, out: &mut String) {
    match c {
        '\\' => out.push_str("\\backslash "),
        '{' | '}' | '$' | '&' | '#' | '%' | '_' => {
            out.push('\\');
            out.push(c);
        }
        '^' => out.push_str("\\hat{}"),
        '~' => out.push_str("\\sim "),
        '∗' => out.push('*'),
        c => out.push(c),
    }
}

/// TeX for a run of events. Runs of two or more letters are set upright (`\mathrm{TopK}`).
fn tex(evs: &[Ev]) -> String {
    let mut out = String::new();
    let mut letters = String::new();
    let flush = |letters: &mut String, out: &mut String| {
        if letters.chars().count() >= 2 {
            out.push_str(&format!("\\mathrm{{{letters}}}"));
        } else {
            out.push_str(letters);
        }
        letters.clear();
    };
    for e in evs {
        match e {
            Ev::Char(g)
                if g.accent.is_none() && g.text.chars().all(|c| c.is_ascii_alphabetic()) =>
            {
                letters.push_str(&g.text);
            }
            Ev::Char(g) => {
                flush(&mut letters, &mut out);
                let mut s = String::new();
                for c in g.text.chars() {
                    tex_char(c, &mut s);
                }
                match g
                    .accent
                    .and_then(|a| ACCENTS.iter().find(|(c, ..)| *c == a))
                {
                    Some((_, cmd, _)) => out.push_str(&format!("\\{cmd}{{{s}}}")),
                    None => out.push_str(&s),
                }
            }
            Ev::Open(sub) => {
                flush(&mut letters, &mut out);
                out.push_str(if *sub { "_{" } else { "^{" });
            }
            Ev::Close => {
                flush(&mut letters, &mut out);
                out.push('}');
            }
        }
    }
    flush(&mut letters, &mut out);
    out
}

fn plain_glyph(g: &Glyph, out: &mut String) {
    out.push_str(&g.text);
    if let Some((.., comb)) = g
        .accent
        .and_then(|a| ACCENTS.iter().find(|(c, ..)| *c == a))
    {
        out.push(*comb);
    }
}

/// How a line is written out.
enum Style {
    /// Text, with words that carry scripts turned into math placeholders.
    Text,
    /// Plain characters; scripts are marked with `_` and `^` (code blocks).
    Code,
}

fn render(glyphs: &[Glyph], line: &Line, style: Style, b: &mut Builder) -> String {
    let mut out: Vec<String> = Vec::new();
    for w in words(glyphs, line.size) {
        let evs = script_events(&w, line.y, line.size);
        let accented = w.iter().any(|g| g.accent.is_some());
        // A footnote or affiliation mark after a word (`Vaswani∗`, `Lin1,2`) is not math.
        let is_mark = |e: &Ev| match e {
            Ev::Char(g) => g.text.chars().all(|c| {
                c.is_ascii_digit() || matches!(c, ',' | '∗' | '*' | '†' | '‡' | '§' | '¶')
            }),
            _ => true,
        };
        let plain =
            |e: &Ev| matches!(e, Ev::Char(g) if !g.text.chars().any(|c| MATH_SIGNS.contains(c)));
        let letters = |evs: &[Ev]| {
            evs.iter()
                .filter(|e| matches!(e, Ev::Char(g) if g.text.chars().all(char::is_alphabetic)))
                .count()
        };
        // `word¹`, `word.¹` or `¹word`: one superscript of marks at either end.
        let opens: Vec<usize> = evs
            .iter()
            .enumerate()
            .filter(|(_, e)| matches!(e, Ev::Open(_)))
            .map(|(k, _)| k)
            .collect();
        let close = evs.iter().position(|e| matches!(e, Ev::Close));
        let mark = match (opens.as_slice(), close) {
            ([k], _) if matches!(evs[*k], Ev::Open(false)) && *k >= 2 => {
                evs[..*k].iter().all(plain)
                    && letters(&evs[..*k]) >= 2
                    && evs[*k..].iter().all(is_mark)
            }
            ([0], Some(c)) if matches!(evs[0], Ev::Open(false)) => {
                evs[..c].iter().all(is_mark)
                    && evs[c + 1..].iter().all(plain)
                    && letters(&evs[c + 1..]) >= 2
            }
            _ => false,
        };
        match style {
            Style::Text if mark => {
                // Keep the word, drop the mark.
                let mut depth = 0;
                let mut word = String::new();
                for e in &evs {
                    match e {
                        Ev::Open(_) => depth += 1,
                        Ev::Close => depth -= 1,
                        Ev::Char(g) if depth == 0 => word.push_str(&g.text),
                        _ => {}
                    }
                }
                out.push(word);
            }
            Style::Text if has_script(&evs) || accented => {
                // Keep sentence punctuation outside the math.
                let mut cut = evs.len();
                let count = |evs: &[Ev], c: &str| {
                    evs.iter()
                        .filter(|e| matches!(e, Ev::Char(g) if g.text == c))
                        .count()
                };
                while cut > 0 {
                    match evs[cut - 1] {
                        Ev::Char(g) if matches!(g.text.as_str(), "." | "," | ";" | ":") => cut -= 1,
                        // A closing parenthesis opened before the math.
                        Ev::Char(g)
                            if g.text == ")"
                                && count(&evs[..cut], ")") > count(&evs[..cut], "(") =>
                        {
                            cut -= 1
                        }
                        _ => break,
                    }
                }
                let tail: String = evs[cut..]
                    .iter()
                    .filter_map(|e| match e {
                        Ev::Char(g) => Some(g.text.as_str()),
                        _ => None,
                    })
                    .collect();
                out.push(format!("{}{tail}", b.math(&tex(&evs[..cut]))));
            }
            Style::Code if has_script(&evs) => {
                let mut s = String::new();
                for e in &evs {
                    match e {
                        Ev::Open(true) => s.push('_'),
                        Ev::Open(false) => s.push('^'),
                        Ev::Close => {}
                        Ev::Char(g) => plain_glyph(g, &mut s),
                    }
                }
                out.push(s);
            }
            _ => {
                let mut s = String::new();
                for g in &w {
                    plain_glyph(g, &mut s);
                }
                out.push(s);
            }
        }
    }
    out.join(" ")
}

/// TeX for a whole line or part of one (display equations).
fn line_tex(glyphs: &[Glyph], line: &Line) -> String {
    let mut out = String::new();
    for w in words(glyphs, line.size) {
        let t = tex(&script_events(&w, line.y, line.size));
        let joins_words = out.ends_with(|c: char| c.is_alphanumeric() || c == '}')
            && t.starts_with(|c: char| c.is_alphanumeric() || c == '\\');
        if joins_words {
            out.push_str("\\ ");
        } else if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&t);
    }
    out
}

static EQ_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s\((\d{1,3}[a-z]?)\)$").unwrap());
static CODE_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d{1,2})\s").unwrap());

const MATH_SIGNS: &str = "=≤≥<>∈∉⊆⊂⊇⊃∪∩∑∏∫←→⇒⇔≈≃≡∝∀∃×·∥∇∂±∓≪≫";

fn plain_text(l: &Line) -> String {
    l.plain(0..l.glyphs.len())
}

/// A numbered display equation: the line ends with `(n)` and is mostly symbols.
fn equation_number(l: &Line) -> Option<(String, usize)> {
    let text = plain_text(l);
    let caps = EQ_NUMBER.captures(&text)?;
    let before = &text[..caps.get(0)?.start()];
    let prose = before
        .split_whitespace()
        .filter(|w| w.len() >= 3 && w.chars().all(|c| c.is_lowercase()))
        .count();
    let scripted = has_script(&script_events(&l.glyphs, l.y, l.size));
    if prose > 3 || !(scripted || before.chars().any(|c| MATH_SIGNS.contains(c))) {
        return None;
    }
    // Glyphs of the number: everything from the last "(" on.
    let start = l.glyphs.iter().rposition(|g| g.text.starts_with('('))?;
    Some((caps[1].to_string(), start))
}

fn is_numeric_cell(s: &str) -> bool {
    let core: String = s
        .chars()
        .filter(|c| {
            !c.is_whitespace()
                && !matches!(
                    c,
                    '%' | '±' | '+' | '-' | '−' | '×' | '*' | '†' | '‡' | ',' | '/' | '∗'
                )
        })
        .collect();
    !core.is_empty()
        && core.chars().filter(|c| c.is_ascii_digit()).count() * 2 >= core.chars().count()
}

/// Length of a table starting at `lines[i]`: rows with two or more cells, mostly numbers.
fn table_len(lines: &[Line]) -> usize {
    let mut n = 0;
    let mut wide_rows = 0;
    let (mut cells, mut numeric) = (0usize, 0usize);
    while n < lines.len() {
        let l = &lines[n];
        if n > 0 && (l.y - lines[n - 1].y) > 2.2 * l.size {
            break;
        }
        let runs = l.runs();
        let row_ok = runs.len() >= 2;
        // A short label row (a group name) may sit between rows.
        let label = runs.len() == 1
            && plain_text(l).chars().count() <= 40
            && lines
                .get(n + 1)
                .is_some_and(|next| next.runs().len() >= 2 && (next.y - l.y) <= 2.2 * l.size);
        if !row_ok && !(label && n > 0) {
            break;
        }
        if runs.len() >= 3 {
            wide_rows += 1;
        }
        for r in runs {
            cells += 1;
            numeric += usize::from(is_numeric_cell(&l.plain(r)));
        }
        n += 1;
    }
    let two_cols = wide_rows == 0 && numeric * 3 >= cells;
    if n >= 3 && (wide_rows >= 2 || two_cols) && numeric * 4 >= cells {
        n
    } else {
        0
    }
}

fn table(lines: &[Line], b: &mut Builder) -> Block {
    // Columns come from the rows with the most cells; others snap to the nearest column.
    let max = lines.iter().map(|l| l.runs().len()).max().unwrap_or(1);
    let mut spans: Vec<(f64, f64)> = Vec::new();
    for l in lines.iter().filter(|l| l.runs().len() == max) {
        for r in l.runs() {
            let x0 = l.glyphs[r.start].x;
            let x1 = l.glyphs[r].iter().map(|g| g.end).fold(0., f64::max);
            spans.push((x0, x1));
        }
    }
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut cols: Vec<(f64, f64)> = Vec::new();
    for (x0, x1) in spans {
        match cols.last_mut() {
            Some(c) if x0 <= c.1 => c.1 = c.1.max(x1),
            _ => cols.push((x0, x1)),
        }
    }
    let rows = lines
        .iter()
        .map(|l| {
            let mut row = vec![String::new(); cols.len().max(1)];
            // Each word goes to its column, so a run that swallowed several close cells
            // (`50.9 84.7 53.7`) is split again.
            let mut cells: Vec<(usize, Vec<Glyph>)> = Vec::new();
            for w in words(&l.glyphs, l.size) {
                let x0 = w[0].x;
                let x1 = w.iter().map(|g| g.end).fold(0., f64::max);
                let mid = (x0 + x1) / 2.;
                let k = cols
                    .iter()
                    .enumerate()
                    .min_by(|a, b| {
                        let d = |c: &(f64, f64)| {
                            if c.0 <= mid && mid <= c.1 {
                                0.
                            } else {
                                (c.0 - mid).abs().min((c.1 - mid).abs())
                            }
                        };
                        d(a.1).total_cmp(&d(b.1))
                    })
                    .map_or(0, |(k, _)| k);
                match cells.last_mut() {
                    Some((c, g)) if *c == k => {
                        g.extend(w);
                    }
                    _ => cells.push((k, w)),
                }
            }
            for (k, glyphs) in cells {
                let text = render(&glyphs, l, Style::Text, b);
                if row[k].is_empty() {
                    row[k] = text;
                } else {
                    row[k] = format!("{} {text}", row[k]);
                }
            }
            row
        })
        .collect();
    Block::Table {
        caption: None,
        rows,
    }
}

/// Numbered pseudocode: three or more lines in a row that start with increasing small numbers.
fn code_len(lines: &[Line]) -> usize {
    let mut n = 0;
    let mut last = 0u32;
    while n < lines.len() {
        let text = plain_text(&lines[n]);
        let Some(num) = CODE_LINE
            .captures(&text)
            .and_then(|c| c[1].parse::<u32>().ok())
        else {
            break;
        };
        if num <= last || num > last + 3 {
            break;
        }
        last = num;
        n += 1;
    }
    if n >= 3 { n } else { 0 }
}

/// A short line just above or below an equation, inside its span: a fraction's numerator or
/// denominator.
fn fraction_part(eq: &Line, other: &Line) -> bool {
    let dy = (other.y - eq.y).abs();
    dy > 0.2 * eq.size
        && dy < 1.0 * eq.size
        && other.x0() > eq.x0() - 1.
        && other.x1() < eq.x1()
        && other.glyphs.len() <= 40
}

fn equation(lines: &[Line], i: usize, number: (String, usize), used: &mut [bool]) -> Block {
    let eq = &lines[i];
    let body = &eq.glyphs[..number.1];
    let above =
        (i > 0 && !used[i - 1] && fraction_part(eq, &lines[i - 1]) && lines[i - 1].y < eq.y)
            .then(|| i - 1);
    let below = (i + 1 < lines.len() && fraction_part(eq, &lines[i + 1]) && lines[i + 1].y > eq.y)
        .then_some(i + 1);
    let tex = match (above, below) {
        (Some(a), Some(d)) => {
            let (num, den) = (&lines[a], &lines[d]);
            used[a] = true;
            used[d] = true;
            let x0 = num.x0().min(den.x0());
            let x1 = num.x1().max(den.x1());
            let left: Vec<Glyph> = body.iter().filter(|g| g.end <= x0 + 0.5).cloned().collect();
            let right: Vec<Glyph> = body.iter().filter(|g| g.x >= x1 - 0.5).cloned().collect();
            format!(
                "{} \\frac{{{}}}{{{}}} {}",
                line_tex(&left, eq),
                line_tex(&num.glyphs, num),
                line_tex(&den.glyphs, den),
                line_tex(&right, eq)
            )
        }
        _ => line_tex(body, eq),
    };
    Block::Equation {
        tex: vec![format!("{} \\tag{{{}}}", tex.trim(), number.0)],
    }
}

/// The most common left edge of the lines in a region.
fn margin(lines: &[Line]) -> f64 {
    let mut count: HashMap<i64, usize> = HashMap::new();
    for l in lines {
        *count.entry(l.x0().round() as i64).or_default() += 1;
    }
    count
        .into_iter()
        .max_by_key(|(x, n)| (*n, -*x))
        .map_or(0., |(x, _)| x as f64)
}

fn median_pitch(lines: &[Line]) -> f64 {
    let mut d: Vec<f64> = lines
        .windows(2)
        .map(|w| w[1].y - w[0].y)
        .filter(|d| *d > 0.)
        .collect();
    if d.is_empty() {
        return f64::MAX;
    }
    d.sort_by(f64::total_cmp);
    d[d.len() / 2]
}

/// Size of the title on the first page: clearly larger than the body text, in the top half,
/// on at most three lines.
fn title_size(lines: &[Line]) -> Option<f64> {
    let mut count: HashMap<i64, usize> = HashMap::new();
    for l in lines {
        *count.entry((l.size * 2.).round() as i64).or_default() += l.glyphs.len();
    }
    let body = count.into_iter().max_by_key(|(_, n)| *n)?.0 as f64 / 2.;
    let bottom = lines.iter().map(|l| l.y).fold(0., f64::max);
    let top: Vec<&Line> = lines.iter().filter(|l| l.y < bottom / 2.).collect();
    let size = top.iter().map(|l| l.size).fold(0., f64::max);
    let n = top
        .iter()
        .filter(|l| (l.size - size).abs() < 0.05 * size)
        .count();
    (size > 1.2 * body && (1..=3).contains(&n)).then_some(size)
}

/// The last text line written, to decide whether the next one continues its paragraph.
struct Prev {
    y: f64,
    size: f64,
    /// Left edge relative to the region's margin.
    indent: f64,
    ends_sentence: bool,
    region: (usize, Region),
    /// Ended well before the column's right edge.
    short: bool,
    alone: bool,
}

/// Reads every page into items.
pub fn items(pages: Vec<Page>, b: &mut Builder) -> Vec<Item> {
    let mut out = Vec::new();
    let mut prev: Option<Prev> = None;
    let mut first_page = true;
    let mut page_no = 0usize;
    for page in pages {
        let width = if page.width > 0. {
            page.width
        } else {
            page.glyphs.iter().map(|g| g.end).fold(0., f64::max) + 20.
        };
        let height = if page.height > 0. {
            page.height
        } else {
            f64::MAX
        };
        let mut ls = lines(page.glyphs);
        drop_line_numbers(&mut ls);
        // Page numbers in the header or footer.
        ls.retain(|l| {
            let edge = l.y < 0.08 * height || l.y > 0.92 * height;
            !(edge && is_line_number(plain_text(l).trim()))
        });
        let title_size = if std::mem::take(&mut first_page) {
            title_size(&ls)
        } else {
            None
        };
        page_no += 1;
        // The body text size of the page: the size most glyphs have.
        let mut sizes: HashMap<i64, usize> = HashMap::new();
        for l in &ls {
            *sizes.entry((l.size * 2.).round() as i64).or_default() += l.glyphs.len();
        }
        let body_size = sizes
            .into_iter()
            .max_by_key(|(_, n)| *n)
            .map_or(0., |(k, _)| k as f64 / 2.);
        for (region, ls) in reading_order(ls, width) {
            let pitch = median_pitch(&ls);
            let left = margin(&ls);
            let right = ls.iter().map(Line::x1).fold(0., f64::max);
            let mut used = vec![false; ls.len()];
            // Figure labels drawn between two lines of one paragraph: another size, squeezed
            // between neighbours that are one line apart.
            let body = |l: &Line| (l.size - body_size).abs() <= 0.12 * body_size;
            for i in 1..ls.len() {
                let (a, l) = (&ls[i - 1], &ls[i]);
                // The next line of body text after `i`, skipping other labels.
                let Some(c) = ls[i + 1..].iter().find(|c| body(c)) else {
                    continue;
                };
                if body(a) && !body(l) && c.y - a.y <= 1.35 * a.size.max(pitch) {
                    used[i] = true;
                }
            }
            let numbers: Vec<Option<(String, usize)>> = ls.iter().map(equation_number).collect();
            let mut i = 0;
            while i < ls.len() {
                if used[i] {
                    i += 1;
                    continue;
                }
                if let Some(n) = numbers[i].clone() {
                    // A numerator line was written already only if it isn't a fraction part.
                    out.push(Item::Break);
                    out.push(Item::Block(equation(&ls, i, n, &mut used)));
                    out.push(Item::Break);
                    used[i] = true;
                    prev = None;
                    i += 1;
                    continue;
                }
                if i + 1 < ls.len() && numbers[i + 1].is_some() && fraction_part(&ls[i + 1], &ls[i])
                {
                    // Numerator of the next line's fraction.
                    i += 1;
                    continue;
                }
                let t = if code_len(&ls[i..]) > 0 {
                    0
                } else {
                    table_len(&ls[i..])
                };
                if t > 0 {
                    out.push(Item::Break);
                    out.push(Item::Block(table(&ls[i..i + t], b)));
                    out.push(Item::Break);
                    prev = None;
                    i += t;
                    continue;
                }
                let c = code_len(&ls[i..]);
                if c > 0 {
                    let text = ls[i..i + c]
                        .iter()
                        .map(|l| render(&l.glyphs, l, Style::Code, b))
                        .collect::<Vec<_>>()
                        .join("\n");
                    out.push(Item::Break);
                    out.push(Item::Block(Block::Static { text, code: true }));
                    out.push(Item::Break);
                    prev = None;
                    i += c;
                    continue;
                }
                let l = &ls[i];
                if title_size.is_some_and(|t| (l.size - t).abs() < 0.05 * t) {
                    let mut text = render(&l.glyphs, l, Style::Text, b);
                    while i + 1 < ls.len()
                        && (ls[i + 1].size - l.size).abs() < 0.05 * l.size
                        && ls[i + 1].y - ls[i].y < 2. * l.size
                    {
                        i += 1;
                        text = format!("{text} {}", render(&ls[i].glyphs, &ls[i], Style::Text, b));
                    }
                    out.push(Item::Break);
                    out.push(Item::Title(text));
                    out.push(Item::Break);
                    prev = None;
                    i += 1;
                    continue;
                }
                let indent = l.x0() - left;
                // Small, short text is a label inside a figure: shown, never translated or
                // joined to the prose around it.
                // Captions and footnotes are small too, but they are sentences: many words
                // in one run.
                let label = {
                    let t = plain_text(l);
                    l.size < 0.85 * body_size
                        && (l.runs().len() >= 2 || t.split_whitespace().count() <= 6)
                        && !t.starts_with("Figure")
                        && !t.starts_with("Table")
                        && !t.starts_with("Fig.")
                };
                if label {
                    let text = render(&l.glyphs, l, Style::Text, b);
                    out.push(Item::Break);
                    out.push(Item::Block(Block::Static { text, code: false }));
                    out.push(Item::Break);
                    prev = None;
                    i += 1;
                    continue;
                }
                let gap = prev.as_ref().map_or(0., |p| l.y - p.y);
                // A centered or deeply indented line (title, author, label) stands alone, but
                // a title wrapped over two centered lines stays one piece.
                let alone = indent > 4. * l.size;
                let mut wrapped_heading = false;
                let new_para = match &prev {
                    None => false,
                    Some(p) => {
                        let same_size = (p.size - l.size).abs() <= 0.12 * p.size.max(l.size);
                        let shifted = (indent - p.indent).abs() > 0.5 * l.size;
                        let big = l.size > 1.05 * body_size;
                        if p.alone || alone {
                            !(p.alone && alone && same_size && gap > 0. && gap <= 1.8 * l.size)
                        } else if big && same_size && gap > 0. && gap <= 1.6 * l.size {
                            // A section heading wrapped over two lines.
                            wrapped_heading = true;
                            false
                        } else if gap < 0. || p.region != (page_no, region) {
                            // The next column or page: the paragraph goes on when the last line
                            // ran to the edge and this one isn't indented.
                            !same_size || p.short || p.ends_sentence && indent > 0.5 * l.size
                        } else {
                            !same_size
                                || gap > 1.45 * pitch
                                // A first-line indent, or a hanging indent going back out
                                // (references), after a line that ended early.
                                || (shifted && p.short)
                        }
                    }
                };
                if new_para {
                    out.push(Item::Break);
                }
                let text = render(&l.glyphs, l, Style::Text, b);
                let ends_sentence = text.trim_end().ends_with(['.', ':', '?', '!']);
                match out.last_mut() {
                    // Keep a wrapped heading on one line so it is read as one heading.
                    Some(Item::Line(last)) if wrapped_heading => {
                        last.push(' ');
                        last.push_str(&text);
                    }
                    _ => out.push(Item::Line(text)),
                }
                prev = Some(Prev {
                    y: l.y,
                    size: l.size,
                    indent,
                    short: l.x1() < right - 1.5 * l.size,
                    ends_sentence,
                    alone,
                    region: (page_no, region),
                });
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Glyphs for `text`, one per character, 5 units wide each at size 10.
    fn word(x: f64, y: f64, size: f64, text: &str) -> Vec<Glyph> {
        text.chars()
            .enumerate()
            .filter(|(_, c)| *c != ' ')
            .map(|(i, c)| Glyph::new(x + i as f64 * 0.5 * size, y, size, &c.to_string()))
            .collect()
    }

    fn texts(items: &[Item]) -> Vec<String> {
        items
            .iter()
            .map(|i| match i {
                Item::Line(s) | Item::Title(s) => s.clone(),
                Item::Break => "¶".into(),
                Item::Block(b) => format!("{b:?}"),
            })
            .collect()
    }

    #[test]
    fn margin_line_numbers_are_dropped_and_columns_read_in_order() {
        let mut page = Vec::new();
        for k in 0..10 {
            let y = 100. + k as f64 * 12.;
            page.extend(word(10., y, 8., &format!("{:03}", k + 1)));
            page.extend(word(70., y, 10., &format!("left {k}")));
            page.extend(word(320., y, 10., &format!("right {k}")));
            page.extend(word(560., y, 8., &format!("{:03}", k + 51)));
        }
        let mut b = Builder::default();
        let got: Vec<String> = texts(&items(
            vec![Page {
                width: 600.,
                height: 800.,
                glyphs: page,
            }],
            &mut b,
        ))
        .into_iter()
        .filter(|s| s != "¶")
        .collect();
        let mut want: Vec<String> = (0..10).map(|k| format!("left {k}")).collect();
        want.extend((0..10).map(|k| format!("right {k}")));
        assert_eq!(got, want);
    }

    #[test]
    fn subscripts_become_tex() {
        // R_0 = TopK_{K_0, p_d}
        let mut g = vec![
            Glyph::new(0., 100., 10., "R"),
            Glyph::new(5., 101.5, 7., "0"),
        ];
        g.extend(word(20., 100., 10., "TopK"));
        g.push(Glyph::new(40., 101.5, 7., "K"));
        g.push(Glyph::new(43.5, 102.5, 5., "0"));
        g.push(Glyph::new(46., 101.5, 7., ","));
        g.push(Glyph::new(49.5, 101.5, 7., "p"));
        g.push(Glyph::new(53., 102.5, 5., "d"));
        let l = Line::new(g.clone());
        let ws = words(&l.glyphs, l.size);
        assert_eq!(ws.len(), 2);
        assert_eq!(tex(&script_events(&ws[0], l.y, l.size)), "R_{0}");
        assert_eq!(
            tex(&script_events(&ws[1], l.y, l.size)),
            "\\mathrm{TopK}_{K_{0},p_{d}}"
        );
        let mut b = Builder::default();
        assert_eq!(render(&l.glyphs, &l, Style::Text, &mut b), "⟦M0⟧ ⟦M1⟧");
        assert_eq!(b.math, vec!["R_{0}", "\\mathrm{TopK}_{K_{0},p_{d}}"]);
    }

    #[test]
    fn punctuation_stays_outside_math_and_accents_attach() {
        let mut g = vec![
            Glyph::new(0., 100., 10., "x"),
            Glyph::new(5., 101.5, 7., "i"),
        ];
        g.push(Glyph::new(8.5, 100., 10., "."));
        let mut hat = vec![
            Glyph::new(30., 100., 10., "a"),
            Glyph::new(30.2, 100., 10., "ˆ"),
        ];
        attach_accents(&mut hat);
        assert_eq!(hat.len(), 1);
        g.extend(hat);
        let l = Line::new(g);
        let mut b = Builder::default();
        assert_eq!(render(&l.glyphs, &l, Style::Text, &mut b), "⟦M0⟧. ⟦M1⟧");
        assert_eq!(b.math, vec!["x_{i}", "\\hat{a}"]);
    }

    #[test]
    fn aligned_rows_become_a_table() {
        let mut page = Vec::new();
        let rows = [
            ["Method", "F1", "Recall"],
            ["GraphRAG", "43.1", "63.8"],
            ["Mem0", "58.1", "76.3"],
            ["EPB-RAG", "66.6", "93.2"],
        ];
        for (k, row) in rows.iter().enumerate() {
            let y = 100. + k as f64 * 11.;
            for (c, cell) in row.iter().enumerate() {
                page.extend(word(70. + c as f64 * 80., y, 9., cell));
            }
        }
        let mut b = Builder::default();
        let items = items(
            vec![Page {
                width: 600.,
                height: 800.,
                glyphs: page,
            }],
            &mut b,
        );
        let table = items.iter().find_map(|i| match i {
            Item::Block(Block::Table { rows, .. }) => Some(rows.clone()),
            _ => None,
        });
        assert_eq!(
            table.unwrap(),
            rows.iter()
                .map(|r| r.iter().map(|s| s.to_string()).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn numbered_display_equation_and_fraction() {
        let mut page = Vec::new();
        page.extend(word(70., 100., 10., "We report recall"));
        // ER(q) = |E| / |T|, (4)
        page.extend(word(130., 105., 10., "|E|"));
        page.extend(word(70., 112., 10., "ER(q) ="));
        page.extend(word(160., 112., 10., ","));
        page.extend(word(260., 112., 10., "(4)"));
        page.extend(word(130., 119., 10., "|T|"));
        page.extend(word(70., 131., 10., "where E is the set."));
        let mut b = Builder::default();
        let got = items(
            vec![Page {
                width: 600.,
                height: 800.,
                glyphs: page,
            }],
            &mut b,
        );
        let eq = got
            .iter()
            .find_map(|i| match i {
                Item::Block(Block::Equation { tex }) => Some(tex[0].clone()),
                _ => None,
            })
            .expect("equation");
        assert_eq!(eq, "\\mathrm{ER}(q) = \\frac{|E|}{|T|} , \\tag{4}");
        let lines: Vec<String> = texts(&got)
            .into_iter()
            .filter(|s| !s.starts_with("Equation") && s != "¶")
            .collect();
        assert_eq!(lines, vec!["We report recall", "where E is the set."]);
    }

    #[test]
    fn numbered_pseudocode_is_code() {
        let mut page = Vec::new();
        for (k, s) in ["1 A ← B", "2 P ← A", "4 X ← P"].iter().enumerate() {
            page.extend(word(70., 100. + k as f64 * 12., 10., s));
        }
        let mut b = Builder::default();
        let got = items(
            vec![Page {
                width: 600.,
                height: 800.,
                glyphs: page,
            }],
            &mut b,
        );
        assert!(got.contains(&Item::Block(Block::Static {
            text: "1 A ← B\n2 P ← A\n4 X ← P".into(),
            code: true
        })));
    }
}
