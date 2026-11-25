// SPDX-License-Identifier: GPL-3.0-only
//! `tally report`: areas (depth 1), groups (depth 2) and items with their tests (depth 3), as terminal text,
//! Markdown, JSON (`tally-report.v1`) or SVG bars.

use crate::catalogue::{Catalogue, Origin};
use crate::import::{Extent, Share};
use crate::results::Results;
use crate::scan::Site;
use crate::state::{self, Counts, State};
use serde::Serialize;
use std::collections::BTreeMap;
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
    /// Missing, but works another way (evidence `works`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub works: bool,
    /// The item's share, `done/works/total` (evidence `share_column`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share: Option<ShareText>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share: Option<ShareRow>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<ItemRow>,
}

/// An item's share as the evidence wrote it.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub struct ShareText {
    pub done: u64,
    pub works: u64,
    pub total: u64,
}

/// The average share of the items of a group or area that have one: each item counts once, whatever its size.
/// `done` and `works` are fractions (0 to 1), averaged in millionths so the text is the same everywhere.
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq)]
pub struct ShareRow {
    /// Items with a share.
    pub items: usize,
    pub done: f64,
    pub works: f64,
    #[serde(skip)]
    done_ppm: u64,
    #[serde(skip)]
    works_ppm: u64,
}

impl ShareRow {
    fn add(&mut self, s: Share) {
        // In u128: a part of up to u64::MAX units times a million does not fit in u64. The result is at most a
        // million (part <= total), so it fits again.
        #[allow(clippy::cast_possible_truncation)]
        let ppm = |part: u64| {
            ((u128::from(part) * 1_000_000 + u128::from(s.total) / 2) / u128::from(s.total)) as u64
        };
        self.items += 1;
        self.done_ppm += ppm(s.done);
        self.works_ppm += ppm(s.works);
        self.finish();
    }

    fn merge(&mut self, o: &ShareRow) {
        self.items += o.items;
        self.done_ppm += o.done_ppm;
        self.works_ppm += o.works_ppm;
        self.finish();
    }

    /// The averages in millionths.
    fn mean_ppm(&self) -> (u64, u64) {
        let n = self.items as u64;
        if n == 0 {
            return (0, 0);
        }
        ((self.done_ppm + n / 2) / n, (self.works_ppm + n / 2) / n)
    }

    #[allow(clippy::cast_precision_loss)]
    fn finish(&mut self) {
        let (d, w) = self.mean_ppm();
        self.done = d as f64 / 1e6;
        self.works = w as f64 / 1e6;
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct AreaRow {
    pub id: String,
    pub title: String,
    pub counts: Counts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share: Option<ShareRow>,
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
    /// `report.works_label` and `report.share_label`.
    #[serde(skip)]
    pub works_label: String,
    #[serde(skip)]
    pub share_label: String,
}

/// What the `tsv` evidence files that set `works` or `share_column` say, by full item id.
///
/// # Errors
/// When such a file cannot be read or is malformed.
pub fn extents(cat: &Catalogue) -> Result<BTreeMap<String, Extent>, String> {
    let mut out = BTreeMap::new();
    for ev in &cat.settings.evidence {
        if ev.kind != "tsv" || (ev.works.is_empty() && ev.share_column.is_none()) {
            continue;
        }
        let text = std::fs::read_to_string(cat.root.join(&ev.path))
            .map_err(|e| format!("evidence {}: {e}", ev.path))?;
        let in_scope = |id: &str| {
            ev.groups.is_empty() || ev.groups.iter().any(|g| id.starts_with(&format!("{g}.")))
        };
        for (id, x) in crate::import::tsv_extents(&text, ev)
            .map_err(|e| format!("evidence {}: {e}", ev.path))?
        {
            if in_scope(&id) && out.insert(id.clone(), x).is_some() {
                return Err(format!(
                    "evidence {}: `{id}` already has a works state or share from another evidence file",
                    ev.path
                ));
            }
        }
    }
    Ok(out)
}

/// Builds the report data. Depth decides how much is kept: areas only, groups, or items too. `extents` (from
/// [`extents`]) marks missing items that work another way and gives items their share.
#[must_use]
pub fn build(
    cat: &Catalogue,
    sites: &[Site],
    results: Option<&Results>,
    extents: &BTreeMap<String, Extent>,
    o: &Options,
) -> Report {
    let states = state::all(cat, sites, results);
    let mut idx = 0;
    let mut areas: Vec<AreaRow> = Vec::new();
    let mut totals = Counts::default();
    for g in &cat.groups {
        let mut counts = Counts::default();
        let mut share: Option<ShareRow> = None;
        let mut items = Vec::new();
        for it in &g.items {
            let s = &states[idx];
            idx += 1;
            if o.origin.is_some_and(|wanted| wanted != it.origin) || s.state == State::Retired {
                continue;
            }
            counts.add(s.state);
            let x = extents.get(&it.id).copied().unwrap_or_default();
            let works = x.works && s.state == State::Missing;
            if works {
                counts.works += 1;
            }
            if let Some(sh) = x.share.filter(|_| s.state.counted()) {
                share.get_or_insert_with(ShareRow::default).add(sh);
            }
            if o.depth >= 3 && (!o.missing || s.state == State::Missing) {
                items.push(ItemRow {
                    id: it.id.clone(),
                    title: it.title.clone(),
                    origin: it.origin,
                    state: s.state,
                    label: it.label.clone(),
                    works,
                    share: x.share.map(|v| ShareText {
                        done: v.done,
                        works: v.works,
                        total: v.total,
                    }),
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
                share: None,
                groups: Vec::new(),
            });
            areas.len() - 1
        });
        add(&mut areas[pos].counts, &counts);
        if let Some(sh) = &share {
            areas[pos]
                .share
                .get_or_insert_with(ShareRow::default)
                .merge(sh);
        }
        if o.depth >= 2 {
            areas[pos].groups.push(GroupRow {
                id: g.head.id.clone(),
                title: g.head.title.clone(),
                origin: g.head.origin,
                counts,
                share,
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
        works_label: cat.settings.report.works_label.clone(),
        share_label: cat.settings.report.share_label.clone(),
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
    from.works -= c.works;
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
    to.works += c.works;
}

fn bar(part: usize, total: usize, width: usize) -> String {
    let filled = (part * width).checked_div(total).unwrap_or(0);
    format!("{}{}", "#".repeat(filled), ".".repeat(width - filled))
}

fn line(c: &Counts) -> String {
    let works = if c.works > 0 {
        format!("  {:>4} works another way", c.works)
    } else {
        String::new()
    };
    let main = format!(
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
    );
    main + &works
}

/// `  share: 2.3% done, 95.1% works another way (172 items)` for a row with a share.
fn share_line(r: &Report, sh: &ShareRow) -> String {
    let (d, w) = sh.mean_ppm();
    format!(
        "share: {} {} on average, {} {} ({} items)",
        ppm_percent(d),
        r.share_label,
        ppm_percent(w),
        r.works_label,
        thousands(sh.items)
    )
}

/// Millionths as a percentage with one decimal.
fn ppm_percent(ppm: u64) -> String {
    let tenths = (ppm + 500) / 1000;
    format!("{}.{}%", tenths / 10, tenths % 10)
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
        if let Some(sh) = &a.share {
            let _ = writeln!(s, "{:width$}  {}", "", share_line(r, sh));
        }
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

/// `s` with its first letter in upper case.
fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

fn md_escape(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

fn md(r: &Report, o: &Options) -> String {
    let mut s = String::new();
    // The column of items that work another way only appears when some do, so other reports stay as they were.
    let works = r.areas.iter().any(|a| a.counts.works > 0);
    let (head, rule) = if works {
        (
            format!(" {} |", md_escape(&capital(&r.works_label))),
            " ---: |",
        )
    } else {
        (String::new(), "")
    };
    let _ = writeln!(
        s,
        "| | Implemented | Tagged | Passing | Covered | Missing | Gated | n/a, deferred |{head}"
    );
    let _ = writeln!(
        s,
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |{rule}"
    );
    let row = |s: &mut String, name: &str, c: &Counts| {
        let more = if works {
            format!(" {} |", c.works)
        } else {
            String::new()
        };
        let _ = writeln!(
            s,
            "| {name} | {}/{} | {} | {} | {} | {} | {} | {} |{more}",
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
    let shares: Vec<(String, &ShareRow)> = r
        .areas
        .iter()
        .flat_map(|a| {
            let groups = a.groups.iter().filter(|_| o.depth >= 2).filter_map(|g| {
                g.share
                    .as_ref()
                    .map(|sh| (format!("{} (`{}`)", md_escape(&g.title), g.id), sh))
            });
            a.share
                .as_ref()
                .map(|sh| (format!("**{}**", md_escape(&a.title)), sh))
                .into_iter()
                .chain(groups)
        })
        .collect();
    if !shares.is_empty() {
        let _ = writeln!(
            s,
            "\n| Average share | {} | {} | Items |",
            md_escape(&capital(&r.share_label)),
            md_escape(&capital(&r.works_label))
        );
        let _ = writeln!(s, "| --- | ---: | ---: | ---: |");
        for (name, sh) in shares {
            let (done, works) = sh.mean_ppm();
            let _ = writeln!(
                s,
                "| {name} | {} | {} | {} |",
                ppm_percent(done),
                ppm_percent(works),
                sh.items
            );
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
                    let mut state = it.state.as_str().to_string();
                    if it.works {
                        let _ = write!(state, " ({})", r.works_label);
                    }
                    if let Some(sh) = it.share {
                        let _ = write!(state, ", {}/{} {}", sh.done, sh.total, r.share_label);
                    }
                    let _ = writeln!(
                        s,
                        "| {} | `{}` | {} | {} |",
                        md_escape(&it.title),
                        it.id,
                        md_escape(&state),
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
const PASSING: &str = "#d9862b";
const PALE: &str = "#9c6f2c";
const BUILT: &str = "#6a5632";
/// Missing, but works another way: a cool grey-blue, apart from the warm "done" colours.
const WORKS: &str = "#4f6472";
const GAP: &str = "#a77d2a";
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

/// The ids of the missing items of an area that do not work another way either, shortened to fit one line.
fn gap_line(a: &AreaRow) -> Option<String> {
    let ids: Vec<&str> = a
        .groups
        .iter()
        .flat_map(|g| g.items.iter())
        .filter(|i| i.state == State::Missing && !i.works)
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

/// The first row's title in the picture: "All areas", or which areas it does not count, in a few words.
fn headline_title(r: &Report, h: &Headline) -> String {
    if h.left_out.is_empty() {
        return h.title.clone();
    }
    let names: Vec<&str> = h
        .left_out
        .iter()
        .map(|id| {
            r.areas
                .iter()
                .find(|a| &a.id == id)
                .map_or(id.as_str(), |a| a.title.as_str())
        })
        .collect();
    format!("All areas · not counting {}", names.join(", "))
}

/// The segments of one bar, from the most done to the least, each with its own count (not the running total):
/// covered, passing elsewhere, tagged without a pass, implemented without a tag, missing but working another way,
/// missing. Gated items are implemented
/// and never tagged by CI, so they sit in the implemented segment.
fn segments(c: &Counts) -> [(&'static str, usize); 6] {
    [
        (AMBER, c.covered),
        (PASSING, c.passing.saturating_sub(c.covered)),
        (PALE, c.tagged.saturating_sub(c.passing)),
        (BUILT, c.implemented.saturating_sub(c.tagged)),
        (WORKS, c.works),
        (TRACK, c.missing.saturating_sub(c.works)),
    ]
}

/// The second bar of an area with shares: the average done share (each item counts once) and the average share
/// that works another way, with its legend. Returns the last line's y.
fn share_bar(
    body: &mut String,
    r: &Report,
    title: &str,
    sh: &ShareRow,
    y: usize,
    clip: usize,
    labels: &mut Vec<String>,
) -> usize {
    let y = y + 28;
    let (done, works) = sh.mean_ppm();
    let neither = 1_000_000u64.saturating_sub(done + works);
    labels.push(format!(
        "{title}, average share: {} {}, {} {}",
        ppm_percent(done),
        r.share_label,
        ppm_percent(works),
        r.works_label
    ));
    let counts = format!("{} {} on average", ppm_percent(done), r.share_label);
    let room = BAR.saturating_sub(text_px(counts.chars().count(), CHAR_13) + 20);
    let head = format!("{title} · {} share", r.share_label);
    let _ = writeln!(
        body,
        "  <text x=\"20\" y=\"{y}\" {FONT} font-size=\"13\" fill=\"{TEXT}\">{}</text>",
        xml(&fit(&head, room * 10 / CHAR_13))
    );
    let _ = writeln!(
        body,
        "  <text x=\"780\" y=\"{y}\" text-anchor=\"end\" {FONT} font-size=\"13\" fill=\"{AMBER}\">{}</text>",
        xml(&counts)
    );
    let bar_y = y + 8;
    let _ = writeln!(
        body,
        "  <clipPath id=\"c{clip}\"><rect x=\"20\" y=\"{bar_y}\" width=\"{BAR}\" height=\"12\" rx=\"4\"/></clipPath>"
    );
    let _ = writeln!(
        body,
        "  <rect x=\"20\" y=\"{bar_y}\" width=\"{BAR}\" height=\"12\" rx=\"4\" fill=\"{TRACK}\"/>"
    );
    #[allow(clippy::cast_possible_truncation)]
    let at = |ppm: u64| ((ppm * BAR as u64 + 500_000) / 1_000_000) as usize;
    let (x1, x2) = (at(done), at(done + works));
    for (x0, x1, colour) in [(0, x1, PASSING), (x1, x2, WORKS)] {
        if x1 > x0 {
            let _ = writeln!(
                body,
                "  <rect x=\"{}\" y=\"{bar_y}\" width=\"{}\" height=\"12\" fill=\"{colour}\" clip-path=\"url(#c{clip})\"/>",
                20 + x0,
                x1 - x0
            );
        }
    }
    let _ = writeln!(
        body,
        "  <rect x=\"20\" y=\"{bar_y}\" width=\"{BAR}\" height=\"12\" rx=\"4\" fill=\"none\" stroke=\"{EDGE}\" stroke-width=\"1\"/>"
    );
    let mut legend_y = bar_y + 28;
    let mut x = 20;
    let entries = [
        (PASSING, ppm_percent(done), r.share_label.as_str()),
        (WORKS, ppm_percent(works), r.works_label.as_str()),
        (TRACK, ppm_percent(neither), "neither"),
    ];
    for (colour, pct, name) in entries {
        let text = format!("{pct} {name}");
        let w = 16 + text_px(text.chars().count(), CHAR_11);
        if x > 20 && x + w > 780 {
            legend_y += 16;
            x = 20;
        }
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
        x += w + 18;
    }
    let note = format!("average over {} items", thousands(sh.items));
    if x + text_px(note.chars().count(), CHAR_11) > 780 {
        legend_y += 16;
    }
    let _ = writeln!(
        body,
        "  <text x=\"780\" y=\"{legend_y}\" text-anchor=\"end\" {FONT} font-size=\"11\" fill=\"{TEXT}\">{}</text>",
        xml(&note)
    );
    legend_y
}

/// One panel in the style of the nv-rs progress pictures: a dark panel and, for the headline and every area, one
/// stacked bar of all its items: covered (a tagged test passed on Linux, Windows and macOS), passing (on at least one
/// system), tagged (a test names it), implemented without a tag, and missing; the legend under each bar gives the
/// count of each state, and the missing items are listed. Byte-stable: the same report gives the same bytes.
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
        .map(|h| (headline_title(r, h), &h.counts, None, None))
        .chain(
            r.areas
                .iter()
                .map(|a| (a.title.clone(), &a.counts, gap_line(a), a.share.as_ref())),
        );
    let mut clip = 0;
    for (title, c, gaps, share) in rows {
        let bar_id = clip;
        clip += 1;
        y += 28;
        labels.push(format!(
            "{title}: {} items, {} tagged, {} passing, {} covered",
            c.total, c.tagged, c.passing, c.covered
        ));
        let counts = format!(
            "{} {} · {} covered",
            thousands(c.total),
            if c.total == 1 { "item" } else { "items" },
            percent(c.covered, c.total)
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
            "  <clipPath id=\"c{bar_id}\"><rect x=\"20\" y=\"{bar_y}\" width=\"{BAR}\" height=\"12\" rx=\"4\"/></clipPath>"
        );
        let _ = writeln!(
            body,
            "  <rect x=\"20\" y=\"{bar_y}\" width=\"{BAR}\" height=\"12\" rx=\"4\" fill=\"{TRACK}\"/>"
        );
        // Each segment starts where the running count ends, both rounded the same way, so the segments meet exactly
        // and the last one ends at the bar's end.
        let at = |part: usize| (part * BAR + c.total / 2).checked_div(c.total).unwrap_or(0);
        let mut done = 0;
        for (colour, count) in segments(c) {
            let (x0, x1) = (at(done), at(done + count));
            done += count;
            if colour == TRACK || x1 <= x0 {
                continue;
            }
            let _ = writeln!(
                body,
                "  <rect x=\"{}\" y=\"{bar_y}\" width=\"{}\" height=\"12\" fill=\"{colour}\" clip-path=\"url(#c{bar_id})\"/>",
                20 + x0,
                x1 - x0
            );
        }
        let _ = writeln!(
            body,
            "  <rect x=\"20\" y=\"{bar_y}\" width=\"{BAR}\" height=\"12\" rx=\"4\" fill=\"none\" stroke=\"{EDGE}\" stroke-width=\"1\"/>"
        );
        // Legend: one swatch per segment with the count of its state, which takes in the states before it (the bar
        // reads from the left: covered items are also passing, tagged and implemented); missing last. An entry that
        // does not fit goes to a new line.
        let mut legend_y = bar_y + 28;
        let mut x = 20;
        let entries = [
            (AMBER, c.covered, "covered on all 3 systems"),
            (PASSING, c.passing, "passing on any system"),
            (PALE, c.tagged, "tagged"),
            (BUILT, c.implemented, "implemented"),
            (TRACK, c.missing, "missing"),
        ];
        let works_text = format!("of them {}", r.works_label);
        let works_entry = [(WORKS, c.works, works_text.as_str())];
        let works_entry: &[(&str, usize, &str)] = if c.works > 0 { &works_entry } else { &[] };
        for &(colour, count, name) in entries.iter().chain(works_entry) {
            let text = format!("{} {name}", thousands(count));
            let w = 16 + text_px(text.chars().count(), CHAR_11);
            if x > 20 && x + w > 780 {
                legend_y += 16;
                x = 20;
            }
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
            x += w + 18;
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
                "  <text x=\"20\" y=\"{y}\" {FONT} font-size=\"11\" fill=\"{GAP}\">{}</text>",
                xml(&g)
            );
        }
        if let Some(sh) = share {
            y = share_bar(&mut body, r, &title, sh, y, clip, &mut labels);
            clip += 1;
        }
    }
    y += 26;
    let foot = if r.results {
        "covered: a tagged test passed on Linux, Windows and macOS"
    } else {
        "no test results given: nothing is passing or covered yet"
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
