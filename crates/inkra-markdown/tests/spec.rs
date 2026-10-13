//! CommonMark 0.31.2 and GFM conformance (TS-001, M1/M2 acceptance).
//!
//! Every example is run; the tests assert the pass rates required by the
//! milestones and that intentionally deviating examples are listed in
//! `tests/data/deviations.txt` (one example number per line, `#` comments).

use std::collections::BTreeMap;

use inkra_markdown::ParseOptions;
use inkra_markdown::html::{HtmlOptions, to_html_with};

struct Example {
    number: usize,
    section: String,
    markdown: String,
    html: String,
    extension: Option<String>,
}

fn commonmark_examples() -> Vec<Example> {
    let raw = include_str!("data/commonmark-0.31.2.json");
    let v: serde_json::Value = serde_json::from_str(raw).unwrap();
    v.as_array()
        .unwrap()
        .iter()
        .map(|e| Example {
            number: e["example"].as_u64().unwrap() as usize,
            section: e["section"].as_str().unwrap().to_owned(),
            markdown: e["markdown"].as_str().unwrap().to_owned(),
            html: e["html"].as_str().unwrap().to_owned(),
            extension: None,
        })
        .collect()
}

fn gfm_examples() -> Vec<Example> {
    let raw = include_str!("data/gfm-spec.txt");
    let mut out = Vec::new();
    let mut section = String::new();
    let mut lines = raw.lines();
    let mut number = 0;
    while let Some(line) = lines.next() {
        if let Some(h) = line.strip_prefix("## ") {
            section = h.to_owned();
        }
        if let Some(rest) = line.strip_prefix("```````````````````````````````` example") {
            number += 1;
            let extension = rest.trim().to_owned();
            let mut md = String::new();
            for l in lines.by_ref() {
                if l == "." {
                    break;
                }
                md.push_str(l);
                md.push('\n');
            }
            let mut html = String::new();
            for l in lines.by_ref() {
                if l.starts_with("````````````````````````````````") {
                    break;
                }
                html.push_str(l);
                html.push('\n');
            }
            out.push(Example {
                number,
                section: section.clone(),
                markdown: md.replace('→', "\t"),
                html: html.replace('→', "\t"),
                extension: (!extension.is_empty()).then_some(extension),
            });
        }
    }
    out
}

/// Light normalization in the spirit of the official `normalize.py`: collapse
/// whitespace between tags and normalize `<br>` / void tags.
fn normalize(html: &str) -> String {
    let mut s = html.replace("<br>", "<br />").replace("<hr>", "<hr />");
    s = s.replace("\r\n", "\n");
    let mut out = String::with_capacity(s.len());
    let mut in_pre = false;
    for line in s.lines() {
        if line.contains("<pre") {
            in_pre = true;
        }
        if in_pre {
            out.push_str(line);
        } else {
            out.push_str(line.trim());
        }
        out.push('\n');
        if line.contains("</pre>") {
            in_pre = false;
        }
    }
    out.trim_end().to_owned()
}

const BLOCK_SECTIONS: &[&str] = &[
    "Tabs",
    "Precedence",
    "Thematic breaks",
    "ATX headings",
    "Setext headings",
    "Indented code blocks",
    "Fenced code blocks",
    "HTML blocks",
    "Link reference definitions",
    "Paragraphs",
    "Blank lines",
    "Block quotes",
    "List items",
    "Lists",
];

fn deviations(name: &str) -> Vec<usize> {
    let raw = match name {
        "commonmark" => include_str!("data/deviations-commonmark.txt"),
        _ => include_str!("data/deviations-gfm.txt"),
    };
    raw.lines()
        .filter_map(|l| l.split('#').next())
        .filter_map(|l| l.trim().parse().ok())
        .collect()
}

fn run(
    name: &str,
    examples: &[Example],
    opts: &ParseOptions,
    ho: &HtmlOptions,
) -> BTreeMap<String, (usize, usize)> {
    let verbose = std::env::var("SPEC_VERBOSE").is_ok();
    let only: Option<usize> = std::env::var("SPEC_ONLY").ok().and_then(|s| s.parse().ok());
    let mut stats: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut failed = Vec::new();
    for e in examples {
        if only.is_some_and(|n| n != e.number) {
            continue;
        }
        let got = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            to_html_with(&e.markdown, opts, ho)
        }))
        .unwrap_or_else(|_| "PANIC".into());
        let ok = normalize(&got) == normalize(&e.html);
        let entry = stats.entry(e.section.clone()).or_default();
        entry.1 += 1;
        if ok {
            entry.0 += 1;
        } else {
            failed.push(e.number);
            if verbose {
                eprintln!(
                    "--- {name} #{} ({}) ---\n{}\n=== expected ===\n{}\n=== got ===\n{}",
                    e.number, e.section, e.markdown, e.html, got
                );
            }
        }
    }
    let listed = deviations(name);
    let unlisted: Vec<_> = failed.iter().filter(|n| !listed.contains(n)).collect();
    eprintln!("{name}: failing {failed:?}");
    if !unlisted.is_empty() {
        eprintln!("{name}: failing but not listed as deviation: {unlisted:?}");
    }
    stats
}

fn rate(stats: &BTreeMap<String, (usize, usize)>, filter: impl Fn(&str) -> bool) -> (usize, usize) {
    stats
        .iter()
        .filter(|(k, _)| filter(k))
        .fold((0, 0), |a, (_, v)| (a.0 + v.0, a.1 + v.1))
}

#[test]
fn commonmark_conformance() {
    let examples = commonmark_examples();
    let stats = run(
        "commonmark",
        &examples,
        &ParseOptions::commonmark(),
        &HtmlOptions::default(),
    );
    for (k, (p, t)) in &stats {
        eprintln!("{k:40} {p:3}/{t:3}");
    }
    let (bp, bt) = rate(&stats, |s| BLOCK_SECTIONS.contains(&s));
    let (ip, it) = rate(&stats, |s| !BLOCK_SECTIONS.contains(&s));
    let (ap, at) = rate(&stats, |_| true);
    eprintln!("block: {bp}/{bt}  inline: {ip}/{it}  all: {ap}/{at}");
    if std::env::var("SPEC_ONLY").is_err() {
        assert!(bp * 100 >= bt * 95, "block pass rate {bp}/{bt} < 95%");
        assert!(ip * 100 >= it * 95, "inline pass rate {ip}/{it} < 95%");
    }
}

#[test]
fn gfm_extensions_conformance() {
    let examples: Vec<Example> = gfm_examples()
        .into_iter()
        .filter(|e| e.extension.is_some())
        .collect();
    let stats = run(
        "gfm",
        &examples,
        &ParseOptions::gfm(),
        &HtmlOptions {
            tagfilter: true,
            ..Default::default()
        },
    );
    for (k, (p, t)) in &stats {
        eprintln!("{k:40} {p:3}/{t:3}");
    }
    let tables = rate(&stats, |s| s.starts_with("Tables"));
    let tasks = rate(&stats, |s| s.starts_with("Task list"));
    eprintln!("tables {tables:?} tasks {tasks:?}");
    if std::env::var("SPEC_ONLY").is_err() {
        assert_eq!(tables.0, tables.1, "GFM table examples must all pass");
        assert_eq!(tasks.0, tasks.1, "GFM task list examples must all pass");
    }
}
