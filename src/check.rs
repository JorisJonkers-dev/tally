// SPDX-License-Identifier: GPL-3.0-only
//! `tally check`: the hard checks (errors) and the soft ones (warnings).
//!
//! Errors: ids are valid, unique and where their file says; the build.rs reader and the generator copy agree with
//! this tally; pins match; every reference row has an item and every item reference has a row; implementation
//! evidence agrees with the catalogue; tests name only ids that exist, from inside a test, with the full
//! `ids::` path; no test tags more than ten ids without a `covers-table:` reason; no id of the base revision is
//! gone without `retired` or an alias; the covered (and tagged) count does not drop below the ratchet.
//! Warnings: ignored tests that tag ids, new items without a tag.

use crate::catalogue::{self, Catalogue, Declared, GroupFile};
use crate::ids::tally_ids_gen as idg;
use crate::refs;
use crate::results::Results;
use crate::scan::Scan;
use crate::state::{self, Counts};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

/// The generator source this tally ships.
pub const GENERATOR: &str = include_str!("idsgen.rs");

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error,
    Warning,
}

/// One problem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub level: Level,
    pub code: &'static str,
    pub message: String,
}

impl Finding {
    #[must_use]
    pub fn error(code: &'static str, message: String) -> Self {
        Finding {
            level: Level::Error,
            code,
            message,
        }
    }
    #[must_use]
    pub fn warning(code: &'static str, message: String) -> Self {
        Finding {
            level: Level::Warning,
            code,
            message,
        }
    }
}

/// What `check` compares against besides the working tree.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// A git revision whose catalogue is the base: nothing of it may disappear.
    pub base: Option<String>,
    pub results: Option<Results>,
    /// The counts of the last report on main.
    pub ratchet: Option<Counts>,
    /// How far the covered and tagged counts may drop below the ratchet.
    pub allow_drop: usize,
}

/// The most ids one test may tag without a `covers-table:` reason.
pub const BULK_LIMIT: usize = 10;

/// Every id the catalogue defines: live items, retired items and aliases, with where each is defined.
struct Defined {
    live: BTreeMap<String, String>,
    retired: BTreeSet<String>,
    aliases: BTreeMap<String, String>,
}

fn defined(cat: &Catalogue, out: &mut Vec<Finding>) -> Defined {
    let mut live = BTreeMap::new();
    let mut retired = BTreeSet::new();
    let mut aliases = BTreeMap::new();
    let mut every: BTreeMap<String, String> = BTreeMap::new();
    for g in &cat.groups {
        for it in &g.items {
            for (id, what) in
                std::iter::once((&it.id, "item")).chain(it.aliases.iter().map(|a| (a, "alias")))
            {
                if let Some(prev) = every.get(id) {
                    out.push(Finding::error(
                        "id-unique",
                        format!("{}: `{id}` ({what}) is already defined in {prev}", it.file),
                    ));
                } else {
                    every.insert(id.clone(), it.file.clone());
                }
                if what == "alias" && !idg::valid_id(id) {
                    out.push(Finding::error(
                        "id-syntax",
                        format!("{}: alias `{id}` is not a valid id", it.file),
                    ));
                }
            }
            if it.retired.is_some() {
                retired.insert(it.id.clone());
            } else {
                live.insert(it.id.clone(), it.file.clone());
            }
            for a in &it.aliases {
                aliases.insert(a.clone(), it.id.clone());
            }
        }
    }
    // A group id may not also be an item id (`a.b` as a group and as an item of group `a`).
    let group_ids: BTreeSet<&str> = cat.groups.iter().map(|g| g.head.id.as_str()).collect();
    for (id, file) in &every {
        if group_ids.contains(id.as_str()) {
            out.push(Finding::error(
                "id-unique",
                format!("{file}: `{id}` is both a group and an item"),
            ));
        }
    }
    Defined {
        live,
        retired,
        aliases,
    }
}

fn check_ids(cat: &Catalogue, out: &mut Vec<Finding>) {
    let mut group_seen = BTreeMap::new();
    for g in &cat.groups {
        let id = &g.head.id;
        if !idg::valid_id(id) || !id.contains('.') {
            out.push(Finding::error(
                "id-syntax",
                format!("{}: group id `{id}` must be at least two dotted segments of lower-case letters, digits and `_`, no Rust keywords", g.file),
            ));
        }
        let expected = format!(
            "{}/{}.toml",
            catalogue::rel(&cat.root, &cat.dir),
            id.replace('.', "/")
        );
        if g.file != expected {
            out.push(Finding::error(
                "id-path",
                format!("{}: group `{id}` belongs in {expected}", g.file),
            ));
        }
        if let Some(prev) = group_seen.insert(id.clone(), g.file.clone()) {
            out.push(Finding::error(
                "id-unique",
                format!("{}: group `{id}` is already defined in {prev}", g.file),
            ));
        }
        if g.items.is_empty() {
            out.push(Finding::error(
                "group-empty",
                format!("{}: group `{id}` has no items", g.file),
            ));
        }
        if let Some(r) = &g.head.reference
            && !cat.sources.source.iter().any(|s| &s.id == r)
        {
            out.push(Finding::error(
                "source-unknown",
                format!(
                    "{}: reference `{r}` is not in catalogue/sources.toml",
                    g.file
                ),
            ));
        }
        for it in &g.items {
            let local = it.id.rsplit('.').next().unwrap_or("");
            if !idg::valid_segment(local) {
                out.push(Finding::error(
                    "id-syntax",
                    format!("{}: item id `{local}` must be lower-case letters, digits and `_`, starting with a letter, not a Rust keyword", it.file),
                ));
            }
            if matches!(it.declared, Declared::NotApplicable | Declared::Deferred)
                && it.reason.as_deref().is_none_or(|r| r.trim().is_empty())
            {
                out.push(Finding::error(
                    "status-reason",
                    format!(
                        "{}: `{}` is n/a or deferred and needs a `reason`",
                        it.file, it.id
                    ),
                ));
            }
            if it.gated.as_deref().is_some_and(|r| r.trim().is_empty()) {
                out.push(Finding::error(
                    "status-reason",
                    format!("{}: `{}` has an empty `gated` reason", it.file, it.id),
                ));
            }
            if it.retired.as_deref().is_some_and(|r| r.trim().is_empty()) {
                out.push(Finding::error(
                    "status-reason",
                    format!("{}: `{}` has an empty `retired` reason", it.file, it.id),
                ));
            }
        }
    }
}

/// The build.rs reader must see the same ids as the TOML reader, and the generator copy must be this tally's.
fn check_generator(cat: &Catalogue, out: &mut Vec<Finding>) {
    for g in &cat.groups {
        let path = cat.root.join(&g.file);
        match idg::read_group(&path) {
            Err(e) => out.push(Finding::error("generator-reader", e)),
            Ok(lg) => {
                let toml_items = explicit_items(cat, g);
                let line_items: Vec<(String, bool, Vec<String>)> = lg
                    .items
                    .iter()
                    .map(|i| (format!("{}.{}", lg.id, i.id), i.retired, i.aliases.clone()))
                    .collect();
                if lg.id != g.head.id
                    || lg.items_from != g.head.items_from
                    || line_items != toml_items
                {
                    out.push(Finding::error(
                        "generator-reader",
                        format!("{}: the build.rs reader sees other ids than the TOML reader; keep `id`, `retired`, `aliases` and `items_from` on one line each, as plain strings", g.file),
                    ));
                }
            }
        }
    }
    let Some(gen_path) = cat
        .settings
        .ids
        .generator
        .as_deref()
        .filter(|p| !p.is_empty())
    else {
        return;
    };
    match fs::read_to_string(cat.root.join(gen_path)) {
        Err(_) => out.push(Finding::error(
            "generator-copy",
            format!("{gen_path}: missing; write it with `tally ids --generator {gen_path}`"),
        )),
        Ok(text) if text.replace("\r\n", "\n") != GENERATOR.replace("\r\n", "\n") => out.push(Finding::error(
            "generator-copy",
            format!("{gen_path}: differs from the generator of tally {}; write it again with `tally ids --generator {gen_path}`", env!("CARGO_PKG_VERSION")),
        )),
        Ok(_) => {}
    }
}

/// The explicit `[[item]]` entries of a group, as the TOML reader sees them, for the reader comparison.
fn explicit_items(cat: &Catalogue, g: &catalogue::Group) -> Vec<(String, bool, Vec<String>)> {
    let text = fs::read_to_string(cat.root.join(&g.file)).unwrap_or_default();
    let Ok(gf) = toml::from_str::<GroupFile>(&text) else {
        return Vec::new();
    };
    gf.item
        .iter()
        .map(|i| {
            (
                format!("{}.{}", gf.group.id, i.id),
                i.retired.is_some(),
                i.aliases.clone(),
            )
        })
        .collect()
}

fn check_refs(cat: &Catalogue, out: &mut Vec<Finding>) {
    let (findings, lists) = refs::check(cat);
    out.extend(findings);
    let mut claimed: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for it in cat.items() {
        for r in &it.refs {
            let Some((source, key)) = r.split_once(':') else {
                out.push(Finding::error(
                    "ref-syntax",
                    format!(
                        "{}: `{}` ref `{r}` must be `<source>:<key>`",
                        it.file, it.id
                    ),
                ));
                continue;
            };
            match lists.get(source) {
                None if !cat.sources.source.iter().any(|s| s.id == source) => {
                    out.push(Finding::error(
                        "source-unknown",
                        format!("{}: `{}` ref `{r}` names an unknown source", it.file, it.id),
                    ));
                }
                None => {}
                Some(rows) => {
                    if rows.iter().any(|row| row.key == key) {
                        claimed
                            .entry(source.to_string())
                            .or_default()
                            .insert(key.to_string());
                    } else {
                        out.push(Finding::error(
                            "ref-unknown",
                            format!(
                                "{}: `{}` ref `{r}` is not a row of that source's list",
                                it.file, it.id
                            ),
                        ));
                    }
                }
            }
        }
    }
    for (source, rows) in &lists {
        let list = cat
            .sources
            .source
            .iter()
            .find(|s| &s.id == source)
            .map_or_default(|s| s.list.clone());
        for row in rows {
            if !claimed.get(source).is_some_and(|c| c.contains(&row.key)) {
                out.push(Finding::error(
                    "ref-unclaimed",
                    format!("catalogue/{list}: row `{}` ({}) has no catalogue item; add one (status missing, n/a or deferred if need be)", row.key, row.title),
                ));
            }
        }
    }
}

fn check_evidence(cat: &Catalogue, scan: &Scan, out: &mut Vec<Finding>) {
    for it in cat.items() {
        if it.retired.is_some() {
            continue;
        }
        if !it.evidence.is_empty() && !it.declared.is_implemented() {
            out.push(Finding::error(
                "evidence-state",
                format!(
                    "{}: `{}` names implementation evidence but is declared {:?}",
                    it.file, it.id, it.declared
                ),
            ));
        }
        for e in &it.evidence {
            let (path, text) = e
                .split_once('#')
                .map_or((e.as_str(), None), |(p, t)| (p, Some(t)));
            match fs::read_to_string(cat.root.join(path)) {
                Err(_) => out.push(Finding::error(
                    "evidence-file",
                    format!("{}: `{}` evidence `{path}` does not exist", it.file, it.id),
                )),
                Ok(body) => {
                    if let Some(t) = text
                        && !body.contains(t)
                    {
                        out.push(Finding::error(
                            "evidence-text",
                            format!(
                                "{}: `{}` evidence: {path} does not contain `{t}`",
                                it.file, it.id
                            ),
                        ));
                    }
                }
            }
        }
    }
    // A test that tags a missing item says it is implemented.
    let tags = state::tags(&scan.sites);
    for it in cat.items() {
        let Some(tests) = tags.get(&it.id) else {
            continue;
        };
        let first = tests.iter().next().map_or_default(ToString::to_string);
        match it.declared {
            Declared::Missing if it.retired.is_none() => out.push(Finding::error(
                "evidence-tagged-missing",
                format!("{}: `{}` is declared missing but {first} tags it; declare it implemented or partial", it.file, it.id),
            )),
            Declared::NotApplicable | Declared::Deferred => out.push(Finding::warning(
                "tag-not-counted",
                format!("{}: `{}` is n/a or deferred, so the tag in {first} is not counted", it.file, it.id),
            )),
            _ => {}
        }
    }
    for ev in &cat.settings.evidence {
        check_evidence_import(cat, ev, out);
    }
}

fn check_evidence_import(cat: &Catalogue, ev: &catalogue::Evidence, out: &mut Vec<Finding>) {
    let at = format!("catalogue/tally.toml evidence {}", ev.path);
    if let Some((_, issue)) = refs::PLANNED.iter().find(|(k, _)| *k == ev.kind) {
        out.push(Finding::error(
            "evidence-planned",
            format!("{at}: importer `{}` is not built yet ({issue})", ev.kind),
        ));
        return;
    }
    if ev.kind != "tsv" {
        out.push(Finding::error(
            "evidence-kind",
            format!("{at}: unknown importer `{}`", ev.kind),
        ));
        return;
    }
    let text = match fs::read_to_string(cat.root.join(&ev.path)) {
        Ok(t) => t,
        Err(e) => {
            out.push(Finding::error("evidence-file", format!("{at}: {e}")));
            return;
        }
    };
    let mut lines = text.lines();
    let header: Vec<&str> = lines
        .next()
        .unwrap_or("")
        .split('\t')
        .map(str::trim)
        .collect();
    let key_name = ev.key_column.as_deref().unwrap_or("id");
    let Some(key_col) = header.iter().position(|h| *h == key_name) else {
        out.push(Finding::error(
            "evidence-file",
            format!("{at}: no column `{key_name}`"),
        ));
        return;
    };
    let status_col = match &ev.status_column {
        None => None,
        Some(name) => {
            if let Some(c) = header.iter().position(|h| h == name) {
                Some(c)
            } else {
                out.push(Finding::error(
                    "evidence-file",
                    format!("{at}: no column `{name}`"),
                ));
                return;
            }
        }
    };
    let in_scope = |id: &str| {
        ev.groups.is_empty() || ev.groups.iter().any(|g| id.starts_with(&format!("{g}.")))
    };
    let mut says = BTreeSet::new();
    let known: BTreeSet<&str> = cat.items().map(|i| i.id.as_str()).collect();
    for line in lines.filter(|l| !l.trim().is_empty()) {
        let cells: Vec<&str> = line.split('\t').map(str::trim).collect();
        let key = cells.get(key_col).copied().unwrap_or("");
        if !in_scope(key) {
            continue;
        }
        if !known.contains(key) {
            out.push(Finding::error(
                "evidence-import",
                format!("{at}: `{key}` is not a catalogue item"),
            ));
            continue;
        }
        let implemented = status_col.is_none_or(|c| {
            ev.implemented
                .iter()
                .any(|v| Some(v.as_str()) == cells.get(c).copied())
        });
        if implemented {
            says.insert(key.to_string());
        }
    }
    for it in cat
        .items()
        .filter(|i| i.retired.is_none() && in_scope(&i.id))
    {
        let declared = it.declared.is_implemented();
        let import = says.contains(&it.id);
        if declared != import {
            out.push(Finding::error(
                "evidence-import",
                format!(
                    "{}: `{}` is declared {:?} but {} says it is {}",
                    it.file,
                    it.id,
                    it.declared,
                    ev.path,
                    if import {
                        "implemented"
                    } else {
                        "not implemented"
                    }
                ),
            ));
        }
    }
}

fn check_tags(scan: &Scan, d: &Defined, out: &mut Vec<Finding>) {
    out.extend(
        scan.errors
            .iter()
            .map(|e| Finding::error("tag-data", e.clone())),
    );
    let tests: BTreeSet<String> = scan.tests.iter().map(|t| t.test.to_string()).collect();
    for s in &scan.sites {
        let at = format!("{}:{}", s.file, s.line);
        for b in &s.bad {
            out.push(Finding::error(
                "tag-path",
                format!("{at}: `{b}` is not a full `ids::<area>::<group>::<ITEM>` path; name each id by its full path"),
            ));
        }
        if !s.data && !s.is_test {
            out.push(Finding::error(
                "tag-outside-test",
                format!("{at}: covers() is called outside a #[test] function, so tally cannot tell which test it belongs to"),
            ));
        }
        if s.data
            && let Some(t) = &s.test
            && !tests.contains(&t.to_string())
        {
            out.push(Finding::error(
                "tag-data",
                format!(
                    "{at}: the runner test {t} is not a #[test] function in the scanned sources"
                ),
            ));
        }
        if s.ignored && !s.ids.is_empty() {
            out.push(Finding::warning(
                "tag-ignored",
                format!("{at}: the test is #[ignore]d, so its tags never count"),
            ));
        }
        for id in &s.ids {
            if d.live.contains_key(id) {
                continue;
            }
            let why = if d.retired.contains(id) {
                "is retired".to_string()
            } else if let Some(new) = d.aliases.get(id) {
                format!("was renamed to `{new}`")
            } else {
                "is not in the catalogue".to_string()
            };
            out.push(Finding::error("tag-unknown", format!("{at}: `{id}` {why}")));
        }
        if s.ids.len() > BULK_LIMIT && !s.table {
            let how = if s.data {
                "a `covers_table = \"<reason>\"` key"
            } else {
                "a `// covers-table: <reason>` comment in the test"
            };
            out.push(Finding::error(
                "tag-bulk",
                format!("{at}: one test tags {} ids (more than {BULK_LIMIT}); a table-driven test says so with {how}", s.ids.len()),
            ));
        }
    }
}

/// The ids of the catalogue at a git revision: (live, retired, aliases).
type BaseIds = (BTreeSet<String>, BTreeSet<String>, BTreeSet<String>);

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Reads the catalogue at `rev`.
///
/// # Errors
/// When git fails (unknown revision, not a repository).
pub fn base_ids(cat: &Catalogue, rev: &str) -> Result<BaseIds, String> {
    let dir = catalogue::rel(&cat.root, &cat.dir);
    git(
        &cat.root,
        &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
    )?;
    let files = git(
        &cat.root,
        &["ls-tree", "-r", "--name-only", rev, "--", &dir],
    )?;
    let mut live = BTreeSet::new();
    let mut retired = BTreeSet::new();
    let mut aliases = BTreeSet::new();
    for f in files.lines() {
        let Some(inside) = f.strip_prefix(&format!("{dir}/")) else {
            continue;
        };
        if !inside.contains('/')
            || inside.starts_with("ref/")
            || !Path::new(inside)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
        {
            continue;
        }
        let text = git(&cat.root, &["show", &format!("{rev}:{f}")])?;
        let Ok(gf) = toml::from_str::<GroupFile>(&text) else {
            continue;
        };
        for it in &gf.item {
            let id = format!("{}.{}", gf.group.id, it.id);
            if it.retired.is_some() {
                retired.insert(id);
            } else {
                live.insert(id);
            }
            aliases.extend(it.aliases.iter().cloned());
        }
        if let Some(list) = &gf.group.items_from
            && let Ok(t) = git(&cat.root, &["show", &format!("{rev}:{dir}/{list}")])
        {
            for row in catalogue::parse_list(&t).unwrap_or_default() {
                let id = format!("{}.{}", gf.group.id, row.key);
                if !retired.contains(&id) {
                    live.insert(id);
                }
            }
        }
    }
    Ok((live, retired, aliases))
}

fn check_base(cat: &Catalogue, scan: &Scan, d: &Defined, rev: &str, out: &mut Vec<Finding>) {
    let (live, retired, aliases) = match base_ids(cat, rev) {
        Ok(b) => b,
        Err(e) => {
            out.push(Finding::error("base", format!("--base {rev}: {e}")));
            return;
        }
    };
    let now_alias: BTreeSet<&String> = d.aliases.keys().collect();
    for id in live.iter().chain(aliases.iter()) {
        if !d.live.contains_key(id) && !d.retired.contains(id) && !now_alias.contains(id) {
            out.push(Finding::error(
                "removed",
                format!("`{id}` was in the catalogue at {rev} and is gone; keep it with `retired = \"<why>\"`, or list it in the new item's `aliases`"),
            ));
        }
    }
    for id in &retired {
        if !d.retired.contains(id) {
            out.push(Finding::error(
                "unretired",
                format!("`{id}` was retired at {rev}; a retired id is never used again"),
            ));
        }
    }
    let tags = state::tags(&scan.sites);
    for (id, file) in &d.live {
        let was = live.contains(id) || aliases.contains(id);
        let renamed = cat
            .items()
            .find(|i| &i.id == id)
            .is_some_and(|i| i.aliases.iter().any(|a| live.contains(a)));
        if !was && !renamed && !tags.contains_key(id) {
            let declared = cat
                .items()
                .find(|i| &i.id == id)
                .map_or_default(|i| i.declared);
            if declared.is_implemented() {
                out.push(Finding::warning(
                    "untagged-new",
                    format!("{file}: new item `{id}` has no test that tags it"),
                ));
            }
        }
    }
}

fn check_ratchet(counts: &Counts, o: &Options, out: &mut Vec<Finding>) {
    let Some(prev) = &o.ratchet else { return };
    for (what, now, before) in [
        ("covered", counts.covered, prev.covered),
        ("tagged", counts.tagged, prev.tagged),
    ] {
        if now + o.allow_drop < before {
            out.push(Finding::error(
                "ratchet",
                format!("{what} items fell from {before} to {now}; pass --allow-drop {} if that is intended", before - now),
            ));
        }
    }
}

/// Runs every check.
#[must_use]
pub fn run(cat: &Catalogue, scan: &Scan, o: &Options) -> Vec<Finding> {
    let mut out: Vec<Finding> = cat
        .errors
        .iter()
        .map(|e| Finding::error("parse", e.clone()))
        .collect();
    check_ids(cat, &mut out);
    let d = defined(cat, &mut out);
    check_generator(cat, &mut out);
    check_refs(cat, &mut out);
    check_evidence(cat, scan, &mut out);
    check_tags(scan, &d, &mut out);
    if let Some(rev) = &o.base {
        check_base(cat, scan, &d, rev, &mut out);
    }
    let mut counts = Counts::default();
    for s in state::all(cat, &scan.sites, o.results.as_ref()) {
        counts.add(s.state);
    }
    check_ratchet(&counts, o, &mut out);
    out.sort_by_key(|a| a.level);
    out
}
