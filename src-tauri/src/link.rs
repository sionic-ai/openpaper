//! Papers from sites other than arXiv: a pasted paper page link or DOI opens like an arXiv link.
//!
//! The arXiv version is preferred when the paper has one (it reads better and gets a roadmap): an
//! arXiv link the page redirects to, its `citation_arxiv_id`, an arXiv location in OpenAlex, or an
//! arXiv paper with exactly the same title. Otherwise the page's PDF is read like an opened PDF file:
//! the response itself when it is a PDF, the page's `citation_pdf_url` (the Google Scholar tag that
//! ACL Anthology, PMLR, NeurIPS, CVF, JMLR, Nature and most publishers put on paper pages), or an
//! open-access PDF that OpenAlex knows for the DOI. The paper's source is the site's host name.
//! Sites that answer bots with a challenge page (OpenReview) can't be read this way.

use crate::model::{Doc, content_id};
use crate::{arxiv, pdf, search};
use regex::Regex;
use scraper::{Html, Selector};
use std::io::Read;
use std::net::IpAddr;
use std::sync::LazyLock;
use std::time::Duration;
use url::Url;

const MAX_PAGE: u64 = 8 * 1024 * 1024;
const MAX_PDF: u64 = 80 * 1024 * 1024;
const MAX_JSON: u64 = 2 * 1024 * 1024;
const ACCEPT: &str = "text/html,application/xhtml+xml,application/pdf;q=0.9,*/*;q=0.8";
const OPENALEX: &str = "https://api.openalex.org/works/doi:";

pub const NO_PDF: &str =
    "Couldn't find the paper's PDF on that page. Download the PDF and open it with the PDF button";
pub const NOT_A_LINK: &str = "Enter a paper link or DOI";

static DOI: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:doi:\s*)?(10\.\d{4,9}/\S+)$").unwrap());
/// An arXiv ID as the last part of a link's path, as on Hugging Face Papers or alphaXiv
/// (`huggingface.co/papers/2501.00663`).
static ARXIV_IN_PATH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/(\d{4}\.\d{4,5})(?:v\d+)?/?$").unwrap());
static ARXIV_LOCATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)arxiv\.org/(?:abs|pdf)/(\d{4}\.\d{4,5}|[a-z\-]+(?:\.[a-z]{2})?/\d{7})")
        .unwrap()
});

/// What the input points at.
#[derive(Debug, PartialEq)]
enum Target {
    Doi(String),
    Page(Url),
}

/// A DOI (`10.1145/…`, `doi:10.…`, a doi.org link) or a web link to a paper page or PDF.
fn target(input: &str) -> Option<Target> {
    let s = input.trim();
    if let Some(c) = DOI.captures(s) {
        return Some(Target::Doi(c[1].trim_end_matches(['.', ',']).to_string()));
    }
    let with_scheme = if s.contains("://") {
        s.to_string()
    } else if !s.contains(char::is_whitespace) && s.contains('/') {
        format!("https://{s}")
    } else {
        return None;
    };
    let u = Url::parse(&with_scheme).ok()?;
    if !matches!(u.scheme(), "http" | "https") || !public_host(&u) {
        return None;
    }
    // A bare `GPT-4.5/o1` is a search, not a site: a typed host needs a letters-only top-level domain.
    if !s.contains("://") {
        let tld = u.host_str()?.rsplit('.').next().unwrap_or_default();
        if tld.len() < 2 || !tld.chars().all(|c| c.is_ascii_alphabetic()) {
            return None;
        }
    }
    let host = u
        .host_str()?
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    if matches!(host.as_str(), "doi.org" | "dx.doi.org") {
        let doi = percent_decode(u.path().trim_start_matches('/'));
        if DOI.is_match(&doi) {
            return Some(Target::Doi(doi));
        }
    }
    Some(Target::Page(u))
}

/// True for inputs `fetch` takes: a DOI or a link to a site other than arXiv.
pub fn looks_like(input: &str) -> bool {
    arxiv::parse_id(input).is_none() && target(input).is_some()
}

/// Links must not reach this machine or the local network.
fn public_host(u: &Url) -> bool {
    match u.host() {
        Some(url::Host::Domain(d)) => {
            let d = d.to_ascii_lowercase();
            d.contains('.')
                && !d.ends_with(".local")
                && !d.ends_with(".localhost")
                && d != "localhost"
        }
        Some(url::Host::Ipv4(ip)) => public_ip(IpAddr::V4(ip)),
        Some(url::Host::Ipv6(ip)) => public_ip(IpAddr::V6(ip)),
        None => false,
    }
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            !(v.is_private()
                || v.is_loopback()
                || v.is_link_local()
                || v.is_unspecified()
                || v.is_broadcast())
        }
        IpAddr::V6(v) => {
            !(v.is_loopback()
                || v.is_unspecified()
                || (v.segments()[0] & 0xfe00) == 0xfc00
                || (v.segments()[0] & 0xffc0) == 0xfe80)
        }
    }
}

fn percent_decode(s: &str) -> String {
    url::form_urlencoded::parse(format!("x={}", s.replace('+', "%2B")).as_bytes())
        .next()
        .map(|(_, v)| v.into_owned())
        .unwrap_or_else(|| s.to_string())
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .user_agent(concat!(
            "openpaper/",
            env!("CARGO_PKG_VERSION"),
            " (paper reader)"
        ))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::custom(|a| {
            if a.previous().len() >= 8 {
                a.error("too many redirects")
            } else if !public_host(a.url()) {
                a.stop()
            } else {
                a.follow()
            }
        }))
        .build()
        .expect("http client")
}

fn read(resp: reqwest::blocking::Response, cap: u64) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    resp.take(cap + 1)
        .read_to_end(&mut buf)
        .map_err(|_| "Couldn't read the response".to_string())?;
    if buf.len() as u64 > cap {
        return Err("The file is too large".into());
    }
    Ok(buf)
}

fn is_pdf(bytes: &[u8]) -> bool {
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(0);
    bytes[start..].starts_with(b"%PDF")
}

/// The Google Scholar `citation_*` tags of a paper page.
#[derive(Debug, Default, PartialEq)]
struct PageMeta {
    title: String,
    doi: Option<String>,
    arxiv: Option<String>,
    pdf: Option<Url>,
}

fn page_meta(html: &str, base: &Url) -> PageMeta {
    let doc = Html::parse_document(html);
    let sel = Selector::parse("meta[content]").unwrap();
    let mut m = PageMeta::default();
    for e in doc.select(&sel) {
        let v = e.value();
        let Some(name) = v.attr("name").or_else(|| v.attr("property")) else {
            continue;
        };
        let content = v.attr("content").unwrap_or_default().trim();
        if content.is_empty() {
            continue;
        }
        match name.to_ascii_lowercase().as_str() {
            "citation_title" if m.title.is_empty() => {
                m.title = content.split_whitespace().collect::<Vec<_>>().join(" ")
            }
            "citation_doi" if m.doi.is_none() => {
                m.doi = DOI
                    .captures(content.trim_start_matches("https://doi.org/"))
                    .map(|c| c[1].to_string())
            }
            "citation_arxiv_id" if m.arxiv.is_none() => m.arxiv = arxiv::parse_id(content),
            "citation_pdf_url" if m.pdf.is_none() => {
                m.pdf = base.join(content).ok().filter(public_host)
            }
            _ => {}
        }
    }
    m
}

/// What OpenAlex knows about a DOI: its title, an arXiv ID and open-access PDF links.
#[derive(Debug, Default)]
struct Work {
    title: String,
    arxiv: Option<String>,
    pdfs: Vec<Url>,
}

fn openalex(http: &reqwest::blocking::Client, doi: &str) -> Option<Work> {
    let resp = http.get(format!("{OPENALEX}{doi}")).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let w: serde_json::Value = serde_json::from_slice(&read(resp, MAX_JSON).ok()?).ok()?;
    Some(work_of(&w))
}

fn work_of(w: &serde_json::Value) -> Work {
    let mut work = Work {
        title: w["title"].as_str().unwrap_or_default().to_string(),
        ..Default::default()
    };
    let mut locations: Vec<&serde_json::Value> = Vec::new();
    if w["best_oa_location"].is_object() {
        locations.push(&w["best_oa_location"]);
    }
    locations.extend(w["locations"].as_array().into_iter().flatten());
    for l in locations {
        for k in ["landing_page_url", "pdf_url"] {
            let Some(u) = l[k].as_str() else { continue };
            if work.arxiv.is_none() {
                work.arxiv = ARXIV_LOCATION.captures(u).map(|c| c[1].to_string());
            }
            if k == "pdf_url" {
                if let Some(u) = Url::parse(u).ok().filter(public_host) {
                    if !work.pdfs.contains(&u) {
                        work.pdfs.push(u);
                    }
                }
            }
        }
    }
    work
}

fn normalized(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The arXiv paper with exactly this title, if there is one.
fn arxiv_twin(title: &str) -> Option<String> {
    let want = normalized(title);
    if want.len() < 12 {
        return None;
    }
    let phrase: String = title
        .chars()
        .map(|c| if c == '"' { ' ' } else { c })
        .collect();
    let page = search::search_n(&format!("\"{phrase}\""), 0, 5).ok()?;
    page.hits
        .into_iter()
        .find(|h| normalized(&h.title) == want)
        .map(|h| h.id)
}

fn get_pdf(http: &reqwest::blocking::Client, url: &Url) -> Option<Vec<u8>> {
    let resp = http.get(url.clone()).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    read(resp, MAX_PDF).ok().filter(|b| is_pdf(b))
}

fn host_of(u: &Url) -> String {
    u.host_str()
        .unwrap_or_default()
        .trim_start_matches("www.")
        .to_string()
}

fn from_pdf(bytes: &[u8], title: &str, source: &str) -> Result<Doc, String> {
    let mut b = pdf::parse(bytes)?;
    if !title.is_empty() {
        b.title = title.to_string();
    } else if b.title.is_empty() {
        b.title = source.to_string();
    }
    Ok(b.finish(format!("pdf-{}", content_id(bytes)), source))
}

/// Opens a paper from a link or DOI. See the module comment for the order things are tried in.
pub fn fetch(input: &str) -> Result<Doc, String> {
    if let Some(id) = arxiv::parse_id(input) {
        return arxiv::fetch(&id);
    }
    let target = target(input).ok_or_else(|| NOT_A_LINK.to_string())?;
    let http = client();
    let (mut doi, page_url) = match target {
        Target::Doi(d) => (
            Some(d.clone()),
            Url::parse(&format!("https://doi.org/{d}")).unwrap(),
        ),
        Target::Page(u) => (None, u),
    };
    if let Some(c) = ARXIV_IN_PATH.captures(page_url.path()) {
        return arxiv::fetch(&c[1]);
    }
    let mut meta = PageMeta::default();
    let mut source = host_of(&page_url);
    let mut page_error = None;
    // One retry: some sites drop the first connection.
    let get = || {
        http.get(page_url.clone())
            .header(reqwest::header::ACCEPT, ACCEPT)
            .send()
    };
    match get().or_else(|_| get()) {
        Ok(resp) if resp.status().is_success() => {
            let final_url = resp.url().clone();
            source = host_of(&final_url);
            if let Some(id) = arxiv::parse_id(final_url.as_str()) {
                return arxiv::fetch(&id);
            }
            let pdf_type = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.contains("pdf"));
            let body = read(resp, if pdf_type { MAX_PDF } else { MAX_PAGE })?;
            if pdf_type || is_pdf(&body) {
                return from_pdf(&body, "", &source);
            }
            meta = page_meta(&String::from_utf8_lossy(&body), &final_url);
            doi = doi.or(meta.doi.clone());
        }
        Ok(resp) => {
            page_error = Some(format!(
                "Couldn't open that page: HTTP {}",
                resp.status().as_u16()
            ))
        }
        Err(_) => page_error = Some("Couldn't connect to that site".to_string()),
    }
    if let Some(id) = &meta.arxiv {
        return arxiv::fetch(id);
    }
    let work = doi
        .as_deref()
        .and_then(|d| openalex(&http, d))
        .unwrap_or_default();
    if let Some(id) = &work.arxiv {
        return arxiv::fetch(id);
    }
    let title = if meta.title.is_empty() {
        work.title.clone()
    } else {
        meta.title.clone()
    };
    if let Some(id) = arxiv_twin(&title) {
        return arxiv::fetch(&id);
    }
    for url in meta.pdf.iter().chain(work.pdfs.iter()) {
        if let Some(bytes) = get_pdf(&http, url) {
            return from_pdf(&bytes, &title, &source);
        }
    }
    Err(page_error
        .filter(|_| title.is_empty())
        .unwrap_or_else(|| NO_PDF.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_dois_and_links() {
        let doi = |s: &str| Some(Target::Doi(s.into()));
        assert_eq!(
            target("10.18653/v1/2023.acl-long.1"),
            doi("10.18653/v1/2023.acl-long.1")
        );
        assert_eq!(
            target("doi: 10.1145/3442188.3445922"),
            doi("10.1145/3442188.3445922")
        );
        assert_eq!(
            target("https://doi.org/10.1038/s41586-021-03819-2"),
            doi("10.1038/s41586-021-03819-2")
        );
        assert_eq!(
            target("https://dx.doi.org/10.1000%2F182"),
            doi("10.1000/182")
        );
        assert!(matches!(
            target("aclanthology.org/2023.acl-long.1/"),
            Some(Target::Page(_))
        ));
        assert!(matches!(
            target("https://proceedings.mlr.press/v202/kirchenbauer23a.html"),
            Some(Target::Page(_))
        ));
        for no in [
            "attention is all you need",
            "GPT-4.5/o1",
            "llama-3.1/8b",
            "transformer",
            "ftp://x.org/a.pdf",
            "http://localhost/a.pdf",
            "http://127.0.0.1/a",
            "http://192.168.0.2/p.pdf",
            "http://[::1]/a",
            "https://printer.local/x",
        ] {
            assert_eq!(target(no), None, "{no}");
        }
    }

    #[test]
    fn finds_an_arxiv_id_in_a_link_path() {
        let id = |u: &str| {
            ARXIV_IN_PATH
                .captures(Url::parse(u).unwrap().path())
                .map(|c| c[1].to_string())
        };
        assert_eq!(
            id("https://huggingface.co/papers/2501.00663").as_deref(),
            Some("2501.00663")
        );
        assert_eq!(
            id("https://www.alphaxiv.org/abs/2501.00663v2").as_deref(),
            Some("2501.00663")
        );
        assert_eq!(id("https://aclanthology.org/2023.acl-long.1/"), None);
        assert_eq!(id("https://doi.org/10.1038/s41586-021-03819-2"), None);
    }

    #[test]
    fn arxiv_inputs_stay_with_the_arxiv_importer() {
        assert!(!looks_like("1706.03762"));
        assert!(!looks_like("https://arxiv.org/abs/1706.03762"));
        assert!(looks_like(
            "https://openaccess.thecvf.com/content_CVPR_2020/html/x.html"
        ));
        assert!(looks_like("10.1145/3442188.3445922"));
        assert!(!looks_like("test time training"));
    }

    #[test]
    fn reads_scholar_tags() {
        let base = Url::parse("https://aclanthology.org/2023.acl-long.1/").unwrap();
        let html = r#"<html><head>
            <meta content="One Cannot Stand for
              Everyone!" name=citation_title>
            <meta name="citation_pdf_url" content="/2023.acl-long.1.pdf">
            <meta name="citation_doi" content="10.18653/v1/2023.acl-long.1">
            <meta name="citation_pdf_url" content="https://other.example/second.pdf">
        </head></html>"#;
        let m = page_meta(html, &base);
        assert_eq!(m.title, "One Cannot Stand for Everyone!");
        assert_eq!(m.doi.as_deref(), Some("10.18653/v1/2023.acl-long.1"));
        assert_eq!(
            m.pdf.unwrap().as_str(),
            "https://aclanthology.org/2023.acl-long.1.pdf"
        );
        assert_eq!(m.arxiv, None);
        let m = page_meta(
            r#"<meta name="citation_arxiv_id" content="1706.03762"><meta name="citation_pdf_url" content="http://10.0.0.1/a.pdf">"#,
            &base,
        );
        assert_eq!(m.arxiv.as_deref(), Some("1706.03762"));
        assert_eq!(m.pdf, None);
    }

    #[test]
    fn finds_arxiv_and_pdfs_in_openalex() {
        let w = serde_json::json!({
            "title": "A paper",
            "best_oa_location": {"landing_page_url": "https://x.org/p", "pdf_url": "https://x.org/p.pdf"},
            "locations": [
                {"landing_page_url": "https://doi.org/10.1/x", "pdf_url": null},
                {"landing_page_url": "https://arxiv.org/abs/2005.14165v4", "pdf_url": "https://arxiv.org/pdf/2005.14165v4"},
                {"landing_page_url": "https://x.org/p", "pdf_url": "https://x.org/p.pdf"}
            ]
        });
        let work = work_of(&w);
        assert_eq!(work.title, "A paper");
        assert_eq!(work.arxiv.as_deref(), Some("2005.14165"));
        assert_eq!(work.pdfs.len(), 2);
        assert_eq!(work.pdfs[0].as_str(), "https://x.org/p.pdf");
    }

    #[test]
    fn pdf_magic_allows_leading_whitespace() {
        assert!(is_pdf(b"%PDF-1.7\n"));
        assert!(is_pdf(b"\r\n %PDF-1.4"));
        assert!(!is_pdf(b"<!doctype html>"));
    }
}
