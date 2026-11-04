//! arXiv search through the public Atom API (export.arxiv.org/api/query).

use crate::arxiv;
use serde::Serialize;

pub const PAGE: usize = 20;
/// Shown when arXiv answers 429 or 503; callers may wait and try again.
pub const BUSY: &str = "arXiv is busy. Search again in a few seconds";
const MAX_FEED: u64 = 4 * 1024 * 1024;
const ATOM: &str = "http://www.w3.org/2005/Atom";
const ARXIV: &str = "http://arxiv.org/schemas/atom";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    /// Identifier without version, e.g. `1706.03762` or `hep-th/9901001`.
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub summary: String,
    /// `YYYY-MM-DD` of the first version.
    pub published: String,
    pub category: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub total: usize,
    pub start: usize,
    pub hits: Vec<Hit>,
}

/// Builds the `search_query` value: every word must match somewhere (title, abstract, authors…).
/// `"quoted phrases"` stay together.
pub fn build_query(input: &str) -> Option<String> {
    let mut terms = Vec::new();
    let mut rest = input.trim();
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('"') {
            let end = after.find('"').unwrap_or(after.len());
            let phrase: String = after[..end]
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if !phrase.is_empty() {
                terms.push(format!("all:\"{phrase}\""));
            }
            rest = after.get(end + 1..).unwrap_or("").trim_start();
        } else {
            let end = rest
                .find(|c: char| c.is_whitespace() || c == '"')
                .unwrap_or(rest.len());
            let word: String = rest[..end]
                .chars()
                .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '.' | '\''))
                .collect();
            if !word.is_empty() {
                terms.push(format!("all:{word}"));
            }
            rest = rest[end..].trim_start();
        }
    }
    (!terms.is_empty()).then(|| terms.join(" AND "))
}

fn text(node: roxmltree::Node, ns: &str, name: &str) -> String {
    node.children()
        .find(|c| c.has_tag_name((ns, name)))
        .and_then(|c| c.text())
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}

pub(crate) fn strip_version(id: &str) -> &str {
    match id.rfind('v') {
        Some(i) if i > 0 && id[i + 1..].chars().all(|c| c.is_ascii_digit()) && i + 1 < id.len() => {
            &id[..i]
        }
        _ => id,
    }
}

pub fn parse_feed(xml: &str) -> Result<Page, String> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|_| "Couldn't read the search results".to_string())?;
    let feed = doc.root_element();
    let num = |name: &str| {
        feed.children()
            .find(|c| {
                c.tag_name().name() == name
                    && c.tag_name()
                        .namespace()
                        .is_some_and(|n| n.contains("opensearch"))
            })
            .and_then(|c| c.text())
            .and_then(|t| t.trim().parse::<usize>().ok())
            .unwrap_or(0)
    };
    let hits = feed
        .children()
        .filter(|c| c.has_tag_name((ATOM, "entry")))
        .filter_map(|e| {
            let raw = text(e, ATOM, "id");
            let id = arxiv::parse_id(&raw)?;
            Some(Hit {
                id: strip_version(&id).to_string(),
                title: text(e, ATOM, "title"),
                authors: e
                    .children()
                    .filter(|a| a.has_tag_name((ATOM, "author")))
                    .map(|a| text(a, ATOM, "name"))
                    .filter(|n| !n.is_empty())
                    .collect(),
                summary: text(e, ATOM, "summary"),
                published: text(e, ATOM, "published").chars().take(10).collect(),
                category: e
                    .children()
                    .find(|c| c.has_tag_name((ARXIV, "primary_category")))
                    .and_then(|c| c.attribute("term"))
                    .unwrap_or_default()
                    .to_string(),
            })
        })
        .collect();
    Ok(Page {
        total: num("totalResults"),
        start: num("startIndex"),
        hits,
    })
}

/// Searches arXiv. An arXiv ID or URL looks the paper up directly; anything else is a keyword search.
pub fn search(input: &str, start: usize) -> Result<Page, String> {
    search_n(input, start, PAGE)
}

/// Looks several papers up by ID in one request. Unknown IDs are simply absent from the result.
pub fn lookup(ids: &[String]) -> Result<Vec<Hit>, String> {
    let ids: Vec<&str> = ids.iter().map(|i| strip_version(i)).collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut url = url::Url::parse("https://export.arxiv.org/api/query").expect("static url");
    url.query_pairs_mut()
        .append_pair("id_list", &ids.join(","))
        .append_pair("max_results", &ids.len().to_string());
    fetch_page(url).map(|p| p.hits)
}

/// Keyword search with a custom page size.
pub fn search_n(input: &str, start: usize, max: usize) -> Result<Page, String> {
    let mut url = url::Url::parse("https://export.arxiv.org/api/query").expect("static url");
    if let Some(id) = arxiv::parse_id(input) {
        // The API answers 500 for versioned old-style IDs (hep-th/9901001v2), so look up the paper itself.
        url.query_pairs_mut()
            .append_pair("id_list", strip_version(&id));
    } else {
        let q = build_query(input).ok_or_else(|| "Enter a search term".to_string())?;
        url.query_pairs_mut()
            .append_pair("search_query", &q)
            .append_pair("sortBy", "relevance")
            .append_pair("start", &start.to_string())
            .append_pair("max_results", &max.to_string());
    }
    fetch_page(url)
}

fn fetch_page(url: url::Url) -> Result<Page, String> {
    let resp = arxiv::client()
        .get(url)
        .send()
        .map_err(|_| "Couldn't connect to arXiv".to_string())?;
    if !resp.status().is_success() {
        return Err(
            if resp.status().as_u16() == 429 || resp.status().as_u16() == 503 {
                BUSY.into()
            } else {
                format!("arXiv search failed ({})", resp.status().as_u16())
            },
        );
    }
    use std::io::Read;
    let mut body = String::new();
    resp.take(MAX_FEED)
        .read_to_string(&mut body)
        .map_err(|_| "Couldn't read the search results".to_string())?;
    parse_feed(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_words_and_phrases() {
        assert_eq!(
            build_query("  flash attention "),
            Some("all:flash AND all:attention".into())
        );
        assert_eq!(
            build_query("\"sliding  window\" llm"),
            Some("all:\"sliding window\" AND all:llm".into())
        );
        assert_eq!(build_query("rope:(x)"), Some("all:ropex".into()));
        assert_eq!(build_query("  \"\" "), None);
    }

    #[test]
    fn versions() {
        assert_eq!(strip_version("2512.10411v5"), "2512.10411");
        assert_eq!(strip_version("hep-th/9901001v3"), "hep-th/9901001");
        assert_eq!(strip_version("1706.03762"), "1706.03762");
    }

    #[test]
    fn parses_atom_feed() {
        let xml = r#"<?xml version='1.0' encoding='UTF-8'?>
<feed xmlns:opensearch="http://a9.com/-/spec/opensearch/1.1/" xmlns:arxiv="http://arxiv.org/schemas/atom" xmlns="http://www.w3.org/2005/Atom">
  <opensearch:itemsPerPage>2</opensearch:itemsPerPage>
  <opensearch:totalResults>132</opensearch:totalResults>
  <opensearch:startIndex>20</opensearch:startIndex>
  <entry>
    <id>http://arxiv.org/abs/2512.10411v5</id>
    <title>SWAA: Sliding Window
      Attention Adaptation</title>
    <summary>  The quadratic
  complexity. </summary>
    <published>2025-12-11T08:00:00Z</published>
    <arxiv:primary_category term="cs.CL"/>
    <author><name>Ada Lovelace</name></author>
    <author><name>Alan Turing</name></author>
  </entry>
  <entry>
    <id>http://arxiv.org/abs/hep-th/9901001v3</id>
    <title>String Junctions</title>
    <published>1999-01-01T01:01:10Z</published>
    <author><name>Yosuke Imamura</name></author>
  </entry>
</feed>"#;
        let p = parse_feed(xml).unwrap();
        assert_eq!((p.total, p.start, p.hits.len()), (132, 20, 2));
        assert_eq!(
            p.hits[0],
            Hit {
                id: "2512.10411".into(),
                title: "SWAA: Sliding Window Attention Adaptation".into(),
                authors: vec!["Ada Lovelace".into(), "Alan Turing".into()],
                summary: "The quadratic complexity.".into(),
                published: "2025-12-11".into(),
                category: "cs.CL".into(),
            }
        );
        assert_eq!(p.hits[1].id, "hep-th/9901001");
        assert_eq!(p.hits[1].category, "");
    }
}
