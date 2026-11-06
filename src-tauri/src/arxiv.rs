//! arXiv HTML (LaTeXML) import with a PDF fallback for papers without an HTML rendering.

use crate::model::{Block, Builder, Doc, normalize};
use base64::Engine;
use regex::Regex;
use scraper::{ElementRef, Html, Node, Selector};
use std::sync::LazyLock;
use std::time::Duration;
use url::Url;

const MAX_HTML: usize = 30 * 1024 * 1024;
const MAX_PDF: usize = 80 * 1024 * 1024;
const MAX_IMAGE: usize = 10 * 1024 * 1024;

static NEW_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[/:\s])(\d{4}\.\d{4,5}(?:v\d+)?)(?:\.pdf)?/?$").unwrap()
});
static OLD_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[/:\s])([a-z\-]+(?:\.[a-z]{2})?/\d{7}(?:v\d+)?)(?:\.pdf)?/?$").unwrap()
});

/// Extracts a canonical arXiv identifier from an ID, `arXiv:` reference, or arxiv.org URL.
pub fn parse_id(input: &str) -> Option<String> {
    let mut s = input.trim().to_string();
    if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("arxiv.org/") {
        if !s.contains("://") {
            s = format!("https://{s}");
        }
        let u = Url::parse(&s).ok()?;
        let host = u.host_str()?.trim_start_matches("www.");
        if !matches!(host, "arxiv.org" | "export.arxiv.org") {
            return None;
        }
        let path = u.path();
        let rest = ["/abs/", "/pdf/", "/html/"]
            .iter()
            .find_map(|p| path.strip_prefix(p))?;
        s = format!("/{rest}");
    } else {
        s = format!(" {s}");
    }
    NEW_ID
        .captures(&s)
        .or_else(|| OLD_ID.captures(&s))
        .map(|c| c[1].to_string())
}

pub fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .user_agent(concat!(
            "openpaper/",
            env!("CARGO_PKG_VERSION"),
            " (paper reader)"
        ))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()
        .expect("http client")
}

fn read_capped(resp: reqwest::blocking::Response, cap: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut buf = Vec::new();
    resp.take(cap as u64 + 1)
        .read_to_end(&mut buf)
        .map_err(|_| "Couldn't read the response".to_string())?;
    if buf.len() > cap {
        return Err("The file is too large".into());
    }
    Ok(buf)
}

pub fn fetch(id: &str) -> Result<Doc, String> {
    let http = client();
    let doc_id = format!("arxiv-{}", id.replace('/', "_"));
    let html_url = format!("https://arxiv.org/html/{id}");
    let resp = http
        .get(&html_url)
        .send()
        .map_err(|_| "Couldn't connect to arXiv".to_string())?;
    if resp.status().is_success() {
        // Image paths look like "1706.03762v7/x1.png" relative to /html/.
        let base = Url::parse("https://arxiv.org/html/paper").unwrap();
        let bytes = read_capped(resp, MAX_HTML)?;
        let html = String::from_utf8_lossy(&bytes);
        if let Some(mut builder) = parse_html(&html, &base) {
            if builder.title.is_empty() {
                // Some renderings have no title element; the API has it.
                if let Ok(hit) = crate::search::lookup(&[id.to_string()]) {
                    builder.title = hit.into_iter().next().map(|h| h.title).unwrap_or_default();
                }
            }
            let mut doc = builder.finish(doc_id, &format!("arXiv:{id}"));
            embed_images(&http, &mut doc);
            return Ok(doc);
        }
    }
    // No LaTeXML rendering: read the PDF instead.
    let resp = http
        .get(format!("https://arxiv.org/pdf/{id}"))
        .send()
        .map_err(|_| "Couldn't connect to arXiv".to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Couldn't find paper {id} on arXiv"));
    }
    let bytes = read_capped(resp, MAX_PDF)?;
    let mut builder = crate::pdf::parse(&bytes)?;
    if builder.title.is_empty() {
        builder.title = format!("arXiv:{id}");
    }
    Ok(builder.finish(doc_id, &format!("arXiv:{id} (PDF)")))
}

fn has(e: &ElementRef, class: &str) -> bool {
    e.value().classes().any(|c| c == class)
}

fn math_tex(e: &ElementRef) -> String {
    if let Some(alt) = e.value().attr("alttext") {
        return alt.to_string();
    }
    let sel = Selector::parse(r#"annotation[encoding="application/x-tex"]"#).unwrap();
    e.select(&sel)
        .next()
        .map(|a| a.text().collect())
        .unwrap_or_default()
}

const SKIP_CLASSES: &[&str] = &[
    "ltx_authors",
    "ltx_dates",
    "ltx_page_footer",
    "ltx_note",
    "ltx_TOC",
    "ltx_role_affiliation",
    "ltx_classification",
];

fn skipped(e: &ElementRef) -> bool {
    matches!(
        e.value().name(),
        "script" | "style" | "nav" | "header" | "footer" | "button" | "noscript"
    ) || SKIP_CLASSES.iter().any(|c| has(e, c))
}

/// Flattens inline content, replacing math with `⟦M<n>⟧` placeholders and dropping footnotes.
fn inline(b: &mut Builder, e: ElementRef, out: &mut String) {
    for child in e.children() {
        match child.value() {
            Node::Text(t) => out.push_str(t),
            Node::Element(_) => {
                let c = ElementRef::wrap(child).unwrap();
                if has(&c, "ltx_note") {
                    continue;
                }
                match c.value().name() {
                    "math" => {
                        let token = b.math(&math_tex(&c));
                        out.push(' ');
                        out.push_str(&token);
                        out.push(' ');
                    }
                    "br" => out.push(' '),
                    _ => inline(b, c, out),
                }
            }
            _ => {}
        }
    }
}

fn inline_text(b: &mut Builder, e: ElementRef) -> String {
    let mut s = String::new();
    inline(b, e, &mut s);
    // Placeholders were padded with spaces; tighten them before punctuation.
    static TIGHT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(⟧)\s+([.,;:)\]])").unwrap());
    let s = normalize(&s);
    TIGHT.replace_all(&s, "$1$2").into_owned()
}

fn walk(b: &mut Builder, e: ElementRef, base: &Url) {
    for child in e.children() {
        if let Some(c) = ElementRef::wrap(child) {
            block(b, c, base);
        }
    }
}

fn block(b: &mut Builder, e: ElementRef, base: &Url) {
    if skipped(&e) {
        return;
    }
    let name = e.value().name();
    if let Some(level) = name
        .strip_prefix('h')
        .and_then(|n| n.parse::<u8>().ok())
        .filter(|_| has(&e, "ltx_title"))
    {
        let text = inline_text(b, e);
        if has(&e, "ltx_title_document") {
            b.title = text.clone();
        }
        let level = if has(&e, "ltx_title_abstract") {
            2
        } else {
            level.clamp(1, 4)
        };
        b.heading(level, &text);
        return;
    }
    if name == "table" && (has(&e, "ltx_equation") || has(&e, "ltx_equationgroup")) {
        equation(b, e);
        return;
    }
    if name == "math" && e.value().attr("display") == Some("block") {
        b.blocks.push(Block::Equation {
            tex: vec![normalize(&math_tex(&e))],
        });
        return;
    }
    if has(&e, "ltx_bibliography") {
        bibliography(b, e);
        return;
    }
    if has(&e, "ltx_listing") || name == "pre" {
        let text: String = e.text().collect();
        b.fixed(&text, true);
        return;
    }
    match name {
        "p" => {
            let text = inline_text(b, e);
            b.para(&text);
        }
        "figure" => figure(b, e, base),
        "table" if has(&e, "ltx_tabular") => {
            let rows = table_rows(b, e);
            if !rows.is_empty() {
                b.blocks.push(Block::Table {
                    caption: None,
                    rows,
                });
            }
        }
        "li" => {
            let tag = e
                .children()
                .filter_map(ElementRef::wrap)
                .find(|c| has(c, "ltx_tag"));
            b.prefix = tag.map(|t| normalize(&t.text().collect::<String>()));
            for child in e.children() {
                if let Some(c) = ElementRef::wrap(child)
                    && !has(&c, "ltx_tag")
                {
                    block(b, c, base);
                }
            }
            b.prefix = None;
        }
        _ => walk(b, e, base),
    }
}

fn equation(b: &mut Builder, e: ElementRef) {
    let row_sel = Selector::parse("tr.ltx_eqn_row").unwrap();
    let math_sel = Selector::parse("math").unwrap();
    let tag_sel = Selector::parse(".ltx_tag_equation").unwrap();
    let mut tex = Vec::new();
    for row in e.select(&row_sel) {
        let parts: Vec<String> = row.select(&math_sel).map(|m| math_tex(&m)).collect();
        if parts.is_empty() {
            continue;
        }
        let mut line = normalize(&parts.join(" "));
        if let Some(tag) = row.select(&tag_sel).next() {
            let tag: String = tag.text().collect();
            let tag = tag
                .trim()
                .trim_start_matches('(')
                .trim_end_matches(')')
                .trim();
            if !tag.is_empty() {
                line.push_str(&format!(" \\tag{{{tag}}}"));
            }
        }
        tex.push(line);
    }
    if tex.is_empty() {
        tex = e
            .select(&math_sel)
            .map(|m| normalize(&math_tex(&m)))
            .collect();
    }
    if !tex.is_empty() {
        b.blocks.push(Block::Equation { tex });
    }
}

fn table_rows(b: &mut Builder, e: ElementRef) -> Vec<Vec<String>> {
    let tr = Selector::parse("tr").unwrap();
    let cell = Selector::parse("td, th").unwrap();
    e.select(&tr)
        .map(|row| {
            row.select(&cell)
                .map(|c| inline_text(b, c))
                .collect::<Vec<_>>()
        })
        .filter(|cells| cells.iter().any(|c| !c.is_empty()))
        .collect()
}

fn figure(b: &mut Builder, e: ElementRef, base: &Url) {
    let caption_el = e
        .children()
        .filter_map(ElementRef::wrap)
        .filter(|c| c.value().name() == "figcaption")
        .last();
    let tabular = Selector::parse("table.ltx_tabular").unwrap();
    let img = Selector::parse("img[src], object[data]").unwrap();
    // Tables are captioned above the body in papers; read the caption first so ids follow reading order.
    let caption = caption_el.and_then(|c| {
        let text = inline_text(b, c);
        b.unit(&text)
    });
    if let Some(t) = e.select(&tabular).next() {
        let rows = table_rows(b, t);
        b.blocks.push(Block::Table { caption, rows });
        return;
    }
    let images: Vec<String> = e
        .select(&img)
        .filter_map(|i| i.value().attr("src").or_else(|| i.value().attr("data")))
        .filter_map(|src| {
            if src.starts_with("data:") {
                Some(src.to_string())
            } else {
                base.join(src).ok().map(|u| u.to_string())
            }
        })
        .collect();
    if images.is_empty() && caption.is_none() {
        return;
    }
    b.blocks.push(Block::Figure { images, caption });
}

fn bibliography(b: &mut Builder, e: ElementRef) {
    let title = Selector::parse(".ltx_title").unwrap();
    let item = Selector::parse("li.ltx_bibitem").unwrap();
    let heading = e
        .select(&title)
        .next()
        .map(|t| normalize(&t.text().collect::<String>()))
        .unwrap_or_else(|| "References".into());
    b.heading(2, &heading);
    for li in e.select(&item) {
        let text = inline_text(b, li);
        b.fixed(&text, false);
    }
}

pub fn parse_html(html: &str, base: &Url) -> Option<Builder> {
    let doc = Html::parse_document(html);
    let article = doc
        .select(&Selector::parse("article.ltx_document").unwrap())
        .next()?;
    let mut b = Builder::default();
    walk(&mut b, article, base);
    (b.translatable_count() > 0).then_some(b)
}

fn image_mime(url: &str, header: Option<&str>) -> &'static str {
    let lower = url.to_lowercase();
    match header.unwrap_or("") {
        h if h.starts_with("image/png") => "image/png",
        h if h.starts_with("image/jpeg") => "image/jpeg",
        h if h.starts_with("image/gif") => "image/gif",
        h if h.starts_with("image/svg") => "image/svg+xml",
        h if h.starts_with("image/webp") => "image/webp",
        _ if lower.ends_with(".jpg") || lower.ends_with(".jpeg") => "image/jpeg",
        _ if lower.ends_with(".gif") => "image/gif",
        _ if lower.ends_with(".svg") => "image/svg+xml",
        _ => "image/png",
    }
}

fn download_image(http: &reqwest::blocking::Client, url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    if parsed.scheme() != "https" || parsed.host_str()? != "arxiv.org" {
        return None;
    }
    let resp = http.get(url).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let mime = image_mime(
        url,
        resp.headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
    );
    let bytes = read_capped(resp, MAX_IMAGE).ok()?;
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:{mime};base64,{data}"))
}

/// Inlines figure images as data URIs so a saved paper opens offline.
fn embed_images(http: &reqwest::blocking::Client, doc: &mut Doc) {
    let mut slots: Vec<&mut String> = doc
        .blocks
        .iter_mut()
        .filter_map(|b| match b {
            Block::Figure { images, .. } => Some(images.iter_mut()),
            _ => None,
        })
        .flatten()
        .filter(|s| !s.starts_with("data:"))
        .collect();
    for chunk in slots.chunks_mut(8) {
        std::thread::scope(|scope| {
            for slot in chunk.iter_mut() {
                scope.spawn(move || {
                    if let Some(data) = download_image(http, slot) {
                        **slot = data;
                    }
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Block;

    #[test]
    fn parses_ids_and_urls() {
        for (input, want) in [
            ("1706.03762", Some("1706.03762")),
            ("arXiv:1706.03762v7", Some("1706.03762v7")),
            ("https://arxiv.org/abs/2310.06825", Some("2310.06825")),
            (
                "https://arxiv.org/pdf/2310.06825v1.pdf",
                Some("2310.06825v1"),
            ),
            ("https://arxiv.org/html/2310.06825v1/", Some("2310.06825v1")),
            ("hep-th/9901001", Some("hep-th/9901001")),
            ("https://evil.example/abs/1706.03762", None),
            ("hello", None),
        ] {
            assert_eq!(parse_id(input).as_deref(), want, "{input}");
        }
    }

    const SAMPLE: &str = r#"<html><body><article class="ltx_document">
<h1 class="ltx_title ltx_title_document">A Paper<span class="ltx_note">thanks</span></h1>
<div class="ltx_authors">Someone</div>
<div class="ltx_abstract"><h6 class="ltx_title ltx_title_abstract">Abstract</h6>
<p class="ltx_p">We study <math alttext="x^{2}" display="inline"><mi>x</mi></math>. It works.</p></div>
<section class="ltx_section"><h2 class="ltx_title ltx_title_section"><span class="ltx_tag">1 </span>Intro</h2>
<div class="ltx_para"><p class="ltx_p">See Fig. 1 for details<span class="ltx_note">footnote text</span>.</p>
<table class="ltx_equation ltx_eqn_table"><tbody><tr class="ltx_equation ltx_eqn_row">
<td><math alttext="a=b" display="block"></math></td><td><span class="ltx_tag ltx_tag_equation">(1)</span></td></tr></tbody></table></div>
<ul class="ltx_itemize"><li class="ltx_item"><span class="ltx_tag ltx_tag_item">•</span><div class="ltx_para"><p class="ltx_p">Item one.</p></div></li></ul>
<figure class="ltx_figure"><img src="2301.00001v1/x1.png"><object type="image/svg+xml" data="2301.00001v1/x2.svg"></object><figcaption>Figure 1: A plot.</figcaption></figure>
<figure class="ltx_table"><figcaption>Table 1: Numbers.</figcaption><table class="ltx_tabular"><tr><th>A</th><td>1</td></tr></table></figure>
</section>
<section class="ltx_bibliography"><h2 class="ltx_title">References</h2><ul><li class="ltx_bibitem">[1] Doe. A book.</li></ul></section>
</article></body></html>"#;

    #[test]
    fn parses_latexml_structure() {
        let base = Url::parse("https://arxiv.org/html/2301.00001").unwrap();
        let b = parse_html(SAMPLE, &base).unwrap();
        assert_eq!(b.title, "A Paper");
        assert_eq!(b.math, vec!["x^{2}"]);
        let kinds: Vec<&str> = b
            .blocks
            .iter()
            .map(|b| match b {
                Block::Heading { .. } => "h",
                Block::Para { .. } => "p",
                Block::Static { .. } => "s",
                Block::Figure { .. } => "f",
                Block::Equation { .. } => "e",
                Block::Table { .. } => "t",
            })
            .collect();
        assert_eq!(
            kinds,
            vec!["h", "h", "p", "h", "p", "e", "p", "f", "t", "h", "s"]
        );
        let Block::Para { unit } = &b.blocks[2] else {
            panic!()
        };
        assert_eq!(unit.sentences, vec!["We study ⟦M0⟧.", "It works."]);
        let Block::Para { unit } = &b.blocks[4] else {
            panic!()
        };
        assert_eq!(unit.sentences, vec!["See Fig. 1 for details."]);
        let Block::Equation { tex } = &b.blocks[5] else {
            panic!()
        };
        assert_eq!(tex, &vec!["a=b \\tag{1}".to_string()]);
        let Block::Para { unit } = &b.blocks[6] else {
            panic!()
        };
        assert_eq!(unit.sentences, vec!["• Item one."]);
        let Block::Figure { images, caption } = &b.blocks[7] else {
            panic!()
        };
        assert_eq!(
            images,
            &vec![
                "https://arxiv.org/html/2301.00001v1/x1.png".to_string(),
                "https://arxiv.org/html/2301.00001v1/x2.svg".to_string()
            ]
        );
        assert_eq!(
            caption.as_ref().unwrap().sentences,
            vec!["Figure 1: A plot."]
        );
        let Block::Table { rows, .. } = &b.blocks[8] else {
            panic!()
        };
        assert_eq!(rows, &vec![vec!["A".to_string(), "1".to_string()]]);
    }
}
