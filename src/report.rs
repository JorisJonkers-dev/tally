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

/// The first row of a report: every area, less those `report.leave_out_of_total` names.
#[derive(Clone, Debug, Serialize)]
pub struct Headline {
    pub title: String,
    /// Area ids the counts leave out.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub left_out: Vec<String>,
    pub counts: Counts,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub tool: String,
    pub repo: String,
    pub results: bool,
    /// Every area; the ratchet reads these.
    pub totals: Counts,
    /// What the reports show first; absent when `report.total` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headline: Option<Headline>,
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
    let headline = headline(cat, &totals, &areas);
    Report {
        schema: SCHEMA,
        tool: format!("tally {}", env!("CARGO_PKG_VERSION")),
        repo: cat.name(),
        results: results.is_some(),
        totals,
        headline,
        areas,
    }
}

/// The headline row: the totals less the areas the settings leave out, or nothing when the settings hide it.
fn headline(cat: &Catalogue, totals: &Counts, areas: &[AreaRow]) -> Option<Headline> {
    let settings = &cat.settings.report;
    if !settings.total {
        return None;
    }
    let left: Vec<&AreaRow> = areas
        .iter()
        .filter(|a| settings.leave_out_of_total.contains(&a.id))
        .collect();
    let mut counts = totals.clone();
    for a in &left {
        sub(&mut counts, &a.counts);
    }
    let title = if left.is_empty() {
        "All areas".to_string()
    } else {
        let names: Vec<&str> = left.iter().map(|a| a.title.as_str()).collect();
        format!("All areas except {}", names.join(", "))
    };
    Some(Headline {
        title,
        left_out: left.iter().map(|a| a.id.clone()).collect(),
        counts,
    })
}

fn sub(from: &mut Counts, c: &Counts) {
    from.total -= c.total;
    from.missing -= c.missing;
    from.implemented -= c.implemented;
    from.tagged -= c.tagged;
    from.passing -= c.passing;
    from.covered -= c.covered;
    from.gated -= c.gated;
    from.not_applicable -= c.not_applicable;
    from.deferred -= c.deferred;
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
        .chain(r.headline.iter().map(|h| term_total(h).len()))
        .max()
        .unwrap_or(4)
        .max(5);
    if let Some(h) = &r.headline {
        let _ = writeln!(s, "{:width$}  {}", term_total(h), line(&h.counts));
    }
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

/// `total`, or `total except engine` when areas are left out.
fn term_total(h: &Headline) -> String {
    if h.left_out.is_empty() {
        "total".into()
    } else {
        format!("total except {}", h.left_out.join(", "))
    }
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
    if let Some(h) = &r.headline {
        let name = if h.left_out.is_empty() {
            "**Total**".to_string()
        } else {
            let rest = h.title.strip_prefix("All areas ").unwrap_or(&h.title);
            format!("**Total, {}**", md_escape(rest))
        };
        row(&mut s, &name, &h.counts);
    }
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

/// `1234567` as `1,234,567`.
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `part` of `total` as a percentage with one decimal, computed in integers so the text is the same everywhere.
fn percent(part: usize, total: usize) -> String {
    let tenths = (part * 1000 + total / 2).checked_div(total).unwrap_or(0);
    format!("{}.{}%", tenths / 10, tenths % 10)
}

const FONT: &str = "font-family=\"Consolas, 'Courier New', monospace\"";
const PANEL: &str = "#14110b";
const AMBER: &str = "#f2c14e";
const PALE: &str = "#a77d2a";
const TRACK: &str = "#2b2418";
const EDGE: &str = "#5c4a26";
const TEXT: &str = "#d9c9a3";
const WIDTH: usize = 800;
const BAR: usize = 760;
/// How many characters of the gap list fit on one line at the small font size.
const GAP_CHARS: usize = 112;
/// Width of one character in tenths of a pixel for the monospace font at 13 px and at 11 px (0.6 em).
const CHAR_13: usize = 78;
const CHAR_11: usize = 66;

/// Pixels that `chars` characters take at a character width of `tenths` tenths of a pixel, rounded up.
fn text_px(chars: usize, tenths: usize) -> usize {
    (chars * tenths).div_ceil(10)
}

/// `text` cut to at most `max` characters, with an ellipsis when it was cut.
fn fit(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// The ids of the missing items of an area, shortened to fit one line.
fn gap_line(a: &AreaRow) -> Option<String> {
    let ids: Vec<&str> = a
        .groups
        .iter()
        .flat_map(|g| g.items.iter())
        .filter(|i| i.state == State::Missing)
        .map(|i| i.id.as_str())
        .collect();
    gap_line_of(&ids)
}

/// `gaps: ` and the ids, as many as fit in `GAP_CHARS` characters, then how many more there are.
fn gap_line_of(ids: &[&str]) -> Option<String> {
    if ids.is_empty() {
        return None;
    }
    let mut line = String::from("gaps: ");
    for (n, id) in ids.iter().enumerate() {
        let rest = ids.len() - n;
        let more = format!(" and {rest} more");
        let more_len = if rest > 1 { more.len() } else { 0 };
        if n == 0 {
            // The first id is always shown, cut when it alone is too long.
            line.push_str(&fit(id, GAP_CHARS.saturating_sub(line.len() + more_len)));
            continue;
        }
        if line.len() + 2 + id.len() + more_len > GAP_CHARS {
            line.push_str(&more);
            return Some(line);
        }
        line.push_str(", ");
        line.push_str(id);
    }
    Some(line)
}

/// One panel in the style of the nv-rs progress pictures: a dark panel, a bar per area with covered (solid amber,
/// passed on Linux, Windows and macOS) over implemented (pale), the counts on the right, a legend under each bar and
/// the missing items listed. Byte-stable: the same report gives the same bytes.
fn svg(r: &Report) -> String {
    let mut body = String::new();
    let mut y = 30;
    let mut labels = Vec::new();
    let _ = writeln!(
        body,
        "  <text x=\"20\" y=\"{y}\" {FONT} font-size=\"15\" fill=\"{AMBER}\">{} · FEATURE CATALOGUE</text>",
        xml(&r.repo.to_uppercase())
    );
    let rows = r
        .headline
        .iter()
        .map(|h| (h.title.clone(), &h.counts, None))
        .chain(
            r.areas
                .iter()
                .map(|a| (a.title.clone(), &a.counts, gap_line(a))),
        );
    for (n, (title, c, gaps)) in rows.enumerate() {
        y += 28;
        let width = |part: usize| (part * BAR + c.total / 2).checked_div(c.total).unwrap_or(0);
        let covered = width(c.covered);
        let implemented = width(c.implemented).max(covered);
        let not_covered = c.implemented.saturating_sub(c.covered);
        labels.push(format!(
            "{title} {} of {} implemented, {} covered",
            c.implemented, c.total, c.covered
        ));
        let counts = format!(
            "{} of {} implemented · {} covered",
            thousands(c.implemented),
            thousands(c.total),
            thousands(c.covered)
        );
        // The title ends at least 20 px before the right-aligned counts start.
        let room = BAR.saturating_sub(text_px(counts.chars().count(), CHAR_13) + 20);
        let _ = writeln!(
            body,
            "  <text x=\"20\" y=\"{y}\" {FONT} font-size=\"13\" fill=\"{TEXT}\">{}</text>",
            xml(&fit(&title, room * 10 / CHAR_13))
        );
        let _ = writeln!(
            body,
            "  <text x=\"780\" y=\"{y}\" text-anchor=\"end\" {FONT} font-size=\"13\" fill=\"{AMBER}\">{}</text>",
            xml(&counts)
        );
        let bar_y = y + 8;
        let _ = writeln!(
            body,
            "  <clipPath id=\"c{n}\"><rect x=\"20\" y=\"{bar_y}\" width=\"{BAR}\" height=\"12\" rx=\"4\"/></clipPath>"
        );
        let _ = writeln!(
            body,
            "  <rect x=\"20\" y=\"{bar_y}\" width=\"{BAR}\" height=\"12\" rx=\"4\" fill=\"{TRACK}\"/>"
        );
        if covered > 0 {
            let _ = writeln!(
                body,
                "  <rect x=\"20\" y=\"{bar_y}\" width=\"{covered}\" height=\"12\" fill=\"{AMBER}\" clip-path=\"url(#c{n})\"/>"
            );
        }
        if implemented > covered {
            let _ = writeln!(
                body,
                "  <rect x=\"{}\" y=\"{bar_y}\" width=\"{}\" height=\"12\" fill=\"{PALE}\" clip-path=\"url(#c{n})\"/>",
                20 + covered,
                implemented - covered
            );
        }
        let _ = writeln!(
            body,
            "  <rect x=\"20\" y=\"{bar_y}\" width=\"{BAR}\" height=\"12\" rx=\"4\" fill=\"none\" stroke=\"{EDGE}\" stroke-width=\"1\"/>"
        );
        // Legend: covered, implemented without a three-platform pass, missing; then what is left out of the count.
        let mut legend_y = bar_y + 28;
        let mut x = 20;
        let entries = [
            (
                AMBER,
                format!(
                    "{} covered ({})",
                    thousands(c.covered),
                    percent(c.covered, c.total)
                ),
            ),
            (
                PALE,
                format!(
                    "{} implemented, not covered ({})",
                    thousands(not_covered),
                    percent(not_covered, c.total)
                ),
            ),
            (
                TRACK,
                format!(
                    "{} missing ({})",
                    thousands(c.missing),
                    percent(c.missing, c.total)
                ),
            ),
        ];
        for (colour, text) in entries {
            let _ = writeln!(
                body,
                "  <rect x=\"{x}\" y=\"{}\" width=\"10\" height=\"10\" fill=\"{colour}\" stroke=\"{EDGE}\" stroke-width=\"1\"/>",
                legend_y - 9
            );
            let _ = writeln!(
                body,
                "  <text x=\"{}\" y=\"{legend_y}\" {FONT} font-size=\"11\" fill=\"{TEXT}\">{}</text>",
                x + 16,
                xml(&text)
            );
            x += 16 + text_px(text.chars().count(), CHAR_11) + 20;
        }
        let left_out = c.not_applicable + c.deferred;
        if left_out > 0 || c.gated > 0 {
            let mut note = Vec::new();
            if c.gated > 0 {
                note.push(format!("{} gated", thousands(c.gated)));
            }
            if left_out > 0 {
                note.push(format!("{} n/a or deferred", thousands(left_out)));
            }
            let note = note.join(" · ");
            // The note goes on its own line when it would run into the legend.
            if x + text_px(note.chars().count(), CHAR_11) > 780 {
                legend_y += 16;
            }
            let _ = writeln!(
                body,
                "  <text x=\"780\" y=\"{legend_y}\" text-anchor=\"end\" {FONT} font-size=\"11\" fill=\"{TEXT}\">{}</text>",
                xml(&note)
            );
        }
        y = legend_y;
        if let Some(g) = gaps {
            y += 16;
            let _ = writeln!(
                body,
                "  <text x=\"20\" y=\"{y}\" {FONT} font-size=\"11\" fill=\"{PALE}\">{}</text>",
                xml(&g)
            );
        }
    }
    y += 26;
    let foot = if r.results {
        "covered: a tagged test passed on Linux, Windows and macOS"
    } else {
        "no test results given: nothing is covered yet"
    };
    let _ = writeln!(
        body,
        "  <text x=\"20\" y=\"{y}\" {FONT} font-size=\"11\" fill=\"{EDGE}\">{} · {}</text>",
        xml(foot),
        xml(&r.tool)
    );
    let height = y + 14;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{WIDTH}\" height=\"{height}\" viewBox=\"0 0 {WIDTH} {height}\" role=\"img\" aria-label=\"{}: {}\">",
        xml(&r.repo),
        xml(&labels.join("; "))
    );
    let _ = writeln!(s, "  <title>{} · feature catalogue</title>", xml(&r.repo));
    let _ = writeln!(
        s,
        "  <rect x=\"0\" y=\"0\" width=\"{WIDTH}\" height=\"{height}\" rx=\"10\" fill=\"{PANEL}\"/>"
    );
    s.push_str(&body);
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

#[cfg(test)]
mod tests {
    use super::{GAP_CHARS, fit, gap_line_of, text_px};

    #[test]
    fn fit_keeps_short_text_and_cuts_long_text_with_an_ellipsis() {
        assert_eq!(fit("abc", 3), "abc");
        assert_eq!(fit("abcdef", 4), "abc…");
        assert_eq!(fit("abc", 0), "…");
    }

    #[test]
    fn a_gap_line_always_shows_the_first_missing_id() {
        let ids = ["x".repeat(200), "short".to_string()];
        let line =
            gap_line_of(&ids.iter().map(String::as_str).collect::<Vec<_>>()).expect("a line");
        assert!(line.starts_with("gaps: xxx"), "{line}");
        assert!(line.ends_with("…, short"), "{line}");
        assert!(line.chars().count() <= GAP_CHARS, "{line}");
    }

    #[test]
    fn text_px_rounds_up() {
        assert_eq!(text_px(10, 66), 66);
        assert_eq!(text_px(1, 66), 7);
    }
}
