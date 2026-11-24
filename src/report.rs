// SPDX-License-Identifier: GPL-3.0-only
//! `tally report`: areas (depth 1), groups (depth 2) and items with their tests (depth 3), as terminal text,
//! Markdown, JSON (`tally-report.v1`) or SVG bars.

use crate::catalogue::{Catalogue, Origin};
use crate::results::Results;
use crate::scan::Site;
use crate::state::{self, Counts, State};
use serde::Serialize;
use std::fmt::Write as _;

pub const SCHEMA: &str = "tally-report.v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Term,
    Md,
    Json,
    Svg,
}

/// What to show.
#[derive(Clone, Debug)]
pub struct Options {
    pub depth: u8,
    pub format: Format,
    /// Only items that are missing (depth 3), and only areas and groups that have some.
    pub missing: bool,
    pub origin: Option<Origin>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ItemRow {
    pub id: String,
    pub title: String,
    pub origin: Origin,
    pub state: State,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub tests: Vec<String>,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub platforms: std::collections::BTreeMap<String, bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GroupRow {
    pub id: String,
    pub title: String,
    pub origin: Origin,
    pub counts: Counts,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<ItemRow>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AreaRow {
    pub id: String,
    pub title: String,
    pub counts: Counts,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<GroupRow>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub tool: String,
    pub repo: String,
    pub results: bool,
    pub totals: Counts,
    pub areas: Vec<AreaRow>,
}

/// Builds the report data. Depth decides how much is kept: areas only, groups, or items too.
#[must_use]
pub fn build(cat: &Catalogue, sites: &[Site], results: Option<&Results>, o: &Options) -> Report {
    let states = state::all(cat, sites, results);
    let mut idx = 0;
    let mut areas: Vec<AreaRow> = Vec::new();
    let mut totals = Counts::default();
    for g in &cat.groups {
        let mut counts = Counts::default();
        let mut items = Vec::new();
        for it in &g.items {
            let s = &states[idx];
            idx += 1;
            if o.origin.is_some_and(|wanted| wanted != it.origin) || s.state == State::Retired {
                continue;
            }
            counts.add(s.state);
            if o.depth >= 3 && (!o.missing || s.state == State::Missing) {
                items.push(ItemRow {
                    id: it.id.clone(),
                    title: it.title.clone(),
                    origin: it.origin,
                    state: s.state,
                    label: it.label.clone(),
                    tests: s.tests.iter().map(ToString::to_string).collect(),
                    platforms: s.platforms.clone(),
                });
            }
        }
        if counts == Counts::default() || (o.missing && counts.missing == 0) {
            continue;
        }
        add(&mut totals, &counts);
        let area = g.area().to_string();
        let pos = areas.iter().position(|a| a.id == area).unwrap_or_else(|| {
            let title = cat
                .settings
                .areas
                .get(&area)
                .cloned()
                .unwrap_or_else(|| area.clone());
            areas.push(AreaRow {
                id: area.clone(),
                title,
                counts: Counts::default(),
                groups: Vec::new(),
            });
            areas.len() - 1
        });
        add(&mut areas[pos].counts, &counts);
        if o.depth >= 2 {
            areas[pos].groups.push(GroupRow {
                id: g.head.id.clone(),
                title: g.head.title.clone(),
                origin: g.head.origin,
                counts,
                items,
            });
        }
    }
    Report {
        schema: SCHEMA,
        tool: format!("tally {}", env!("CARGO_PKG_VERSION")),
        repo: cat.name(),
        results: results.is_some(),
        totals,
        areas,
    }
}

fn add(to: &mut Counts, c: &Counts) {
    to.total += c.total;
    to.missing += c.missing;
    to.implemented += c.implemented;
    to.tagged += c.tagged;
    to.passing += c.passing;
    to.covered += c.covered;
    to.gated += c.gated;
    to.not_applicable += c.not_applicable;
    to.deferred += c.deferred;
}

fn bar(part: usize, total: usize, width: usize) -> String {
    let filled = (part * width).checked_div(total).unwrap_or(0);
    format!("{}{}", "#".repeat(filled), ".".repeat(width - filled))
}

fn line(c: &Counts) -> String {
    format!(
        "{:>4}/{:<4} implemented [{}]  {:>4} tagged  {:>4} passing  {:>4} covered  {:>4} missing  {:>3} gated  {:>3} n/a+deferred",
        c.implemented,
        c.total,
        bar(c.implemented, c.total, 20),
        c.tagged,
        c.passing,
        c.covered,
        c.missing,
        c.gated,
        c.not_applicable + c.deferred
    )
}

fn term(r: &Report, o: &Options) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{}: {} items counted{}",
        r.repo,
        r.totals.total,
        if r.results {
            ""
        } else {
            " (no test results given: nothing is passing or covered yet)"
        }
    );
    let width = r
        .areas
        .iter()
        .flat_map(|a| std::iter::once(a.id.len()).chain(a.groups.iter().map(|g| g.id.len() + 2)))
        .max()
        .unwrap_or(4)
        .max(5);
    let _ = writeln!(s, "{:width$}  {}", "total", line(&r.totals));
    for a in &r.areas {
        let _ = writeln!(s, "{:width$}  {}", a.id, line(&a.counts));
        for g in &a.groups {
            let _ = writeln!(s, "{:width$}  {}", format!("  {}", g.id), line(&g.counts));
            for it in &g.items {
                let tests = if it.tests.is_empty() {
                    String::new()
                } else {
                    format!("  <- {}", it.tests.join(", "))
                };
                let _ = writeln!(
                    s,
                    "      {:<11} {}  {}{tests}",
                    it.state.as_str(),
                    it.id,
                    it.title
                );
            }
        }
    }
    let _ = o;
    s
}

fn md_escape(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

fn md(r: &Report, o: &Options) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "| | Implemented | Tagged | Passing | Covered | Missing | Gated | n/a, deferred |"
    );
    let _ = writeln!(
        s,
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |"
    );
    let row = |s: &mut String, name: &str, c: &Counts| {
        let _ = writeln!(
            s,
            "| {name} | {}/{} | {} | {} | {} | {} | {} | {} |",
            c.implemented,
            c.total,
            c.tagged,
            c.passing,
            c.covered,
            c.missing,
            c.gated,
            c.not_applicable + c.deferred
        );
    };
    row(&mut s, "**Total**", &r.totals);
    for a in &r.areas {
        row(&mut s, &format!("**{}**", md_escape(&a.title)), &a.counts);
        if o.depth >= 2 {
            for g in &a.groups {
                row(
                    &mut s,
                    &format!("{} (`{}`)", md_escape(&g.title), g.id),
                    &g.counts,
                );
            }
        }
    }
    if o.depth >= 3 {
        for a in &r.areas {
            for g in &a.groups {
                if g.items.is_empty() {
                    continue;
                }
                let _ = writeln!(s, "\n### {} (`{}`)\n", md_escape(&g.title), g.id);
                let _ = writeln!(s, "| Item | Id | State | Tests |");
                let _ = writeln!(s, "| --- | --- | --- | --- |");
                for it in &g.items {
                    let tests = it
                        .tests
                        .iter()
                        .map(|t| format!("`{t}`"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let _ = writeln!(
                        s,
                        "| {} | `{}` | {} | {} |",
                        md_escape(&it.title),
                        it.id,
                        it.state.as_str(),
                        tests
                    );
                }
            }
        }
    }
    s
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Two bars per area: implemented (pale) under covered (solid), with the gap counts beside them.
fn svg(r: &Report) -> String {
    let row_h = 34;
    let label_w = 220;
    let bar_w = 300;
    let width = label_w + bar_w + 300;
    let height = 40 + row_h * (r.areas.len() + 1);
    let mut s = String::new();
    let _ = writeln!(
        s,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\" font-family=\"sans-serif\" font-size=\"13\" role=\"img\" aria-label=\"{} coverage\">",
        xml(&r.repo)
    );
    let _ = writeln!(
        s,
        "<title>{}: implemented and covered per area</title>",
        xml(&r.repo)
    );
    let _ = writeln!(s, "<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>");
    let _ = writeln!(
        s,
        "<text x=\"8\" y=\"22\" font-weight=\"bold\" fill=\"#1f2328\">{}</text>",
        xml(&r.repo)
    );
    let _ = writeln!(
        s,
        "<text x=\"{label_w}\" y=\"22\" fill=\"#59636e\">pale: implemented, solid: covered (Linux, Windows, macOS)</text>"
    );
    let rows = std::iter::once(("Total".to_string(), &r.totals))
        .chain(r.areas.iter().map(|a| (a.title.clone(), &a.counts)));
    for (n, (title, c)) in rows.enumerate() {
        let y = 40 + n * row_h;
        let w = |part: usize| (part * bar_w).checked_div(c.total).unwrap_or(0);
        let _ = writeln!(
            s,
            "<text x=\"8\" y=\"{}\" fill=\"#1f2328\">{}</text>",
            y + 17,
            xml(&title)
        );
        let _ = writeln!(
            s,
            "<rect x=\"{label_w}\" y=\"{}\" width=\"{bar_w}\" height=\"22\" fill=\"#eef1f4\"/>",
            y + 2
        );
        let _ = writeln!(
            s,
            "<rect x=\"{label_w}\" y=\"{}\" width=\"{}\" height=\"22\" fill=\"#9ecbff\"/>",
            y + 2,
            w(c.implemented)
        );
        let _ = writeln!(
            s,
            "<rect x=\"{label_w}\" y=\"{}\" width=\"{}\" height=\"22\" fill=\"#0969da\"/>",
            y + 2,
            w(c.covered)
        );
        let _ = writeln!(
            s,
            "<text x=\"{}\" y=\"{}\" fill=\"#1f2328\">{}/{} implemented, {} covered, {} missing, {} gated</text>",
            label_w + bar_w + 10,
            y + 17,
            c.implemented,
            c.total,
            c.covered,
            c.missing,
            c.gated
        );
    }
    s.push_str("</svg>\n");
    s
}

/// Renders the report.
///
/// # Panics
/// Never: the report always serializes.
#[must_use]
pub fn render(r: &Report, o: &Options) -> String {
    match o.format {
        Format::Term => term(r, o),
        Format::Md => md(r, o),
        Format::Json => serde_json::to_string_pretty(r).expect("the report serializes") + "\n",
        Format::Svg => svg(r),
    }
}
