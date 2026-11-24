// SPDX-License-Identifier: GPL-3.0-only
//! Reference lists and their pins: `catalogue/sources.toml` names each upstream repository at a release tag and
//! commit, the SHA-256 of the upstream files the list was made from, and the SHA-256 of the list itself.
//!
//! `refs check` works offline: it hashes the lists. `refs fetch` downloads the pinned upstream files (with `curl`,
//! so no HTTP crate is needed), checks their hashes and, for `extractor = "tsv"`, makes the list again.

use crate::catalogue::{Catalogue, RefRow, Source, parse_list};
use crate::check::Finding;
use crate::sha256;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Extractors that exist.
pub const EXTRACTORS: [&str; 2] = ["manual", "tsv"];
/// Extractors that are planned, each with its issue.
pub const PLANNED: [(&str, &str); 3] = [
    (
        "ledger",
        "https://github.com/JorisJonkers-dev/tally/issues/3",
    ),
    (
        "capabilities",
        "https://github.com/JorisJonkers-dev/tally/issues/4",
    ),
    (
        "contracts",
        "https://github.com/JorisJonkers-dev/tally/issues/5",
    ),
];

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && s.chars()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// The pin problems of every source, and the rows of each list that could be read (by source id).
#[must_use]
pub fn check(cat: &Catalogue) -> (Vec<Finding>, BTreeMap<String, Vec<RefRow>>) {
    let mut out = Vec::new();
    let mut lists = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for s in &cat.sources.source {
        let at = format!("catalogue/sources.toml [{}]", s.id);
        if !seen.insert(s.id.clone()) {
            out.push(Finding::error(
                "source-unique",
                format!("{at}: the source id is used twice"),
            ));
        }
        if !is_hex(&s.commit, 40) {
            out.push(Finding::error(
                "pin-commit",
                format!(
                    "{at}: commit `{}` is not 40 lower-case hex digits",
                    s.commit
                ),
            ));
        }
        if s.tag.trim().is_empty() {
            out.push(Finding::error("pin-tag", format!("{at}: no release tag")));
        }
        if !valid_date(&s.read) {
            out.push(Finding::error(
                "pin-read",
                format!("{at}: read date `{}` is not YYYY-MM-DD", s.read),
            ));
        }
        if !s.repo.starts_with("https://") {
            out.push(Finding::error(
                "pin-repo",
                format!("{at}: repo `{}` is not an https URL", s.repo),
            ));
        }
        for (path, hash) in &s.upstream {
            if !is_hex(hash, 64) {
                out.push(Finding::error(
                    "pin-upstream",
                    format!("{at}: upstream `{path}` has no valid SHA-256"),
                ));
            }
        }
        if s.upstream.is_empty() {
            out.push(Finding::error(
                "pin-upstream",
                format!("{at}: no upstream files pinned; name the files the list was made from"),
            ));
        }
        if let Some((_, issue)) = PLANNED.iter().find(|(k, _)| *k == s.extractor) {
            out.push(Finding::error(
                "extractor-planned",
                format!(
                    "{at}: extractor `{}` is not built yet ({issue})",
                    s.extractor
                ),
            ));
        } else if !EXTRACTORS.contains(&s.extractor.as_str()) {
            out.push(Finding::error(
                "extractor-unknown",
                format!("{at}: unknown extractor `{}`", s.extractor),
            ));
        }
        if s.extractor == "tsv" {
            match &s.tsv {
                None => out.push(Finding::error(
                    "extractor-tsv",
                    format!("{at}: extractor `tsv` needs a [source.tsv] table"),
                )),
                Some(t) if !s.upstream.contains_key(&t.from) => out.push(Finding::error(
                    "extractor-tsv",
                    format!(
                        "{at}: tsv.from `{}` is not one of the pinned upstream files",
                        t.from
                    ),
                )),
                Some(_) => {}
            }
        }
        let path = cat.dir.join(&s.list);
        match fs::read(&path) {
            Err(e) => out.push(Finding::error(
                "pin-list",
                format!("{at}: list catalogue/{}: {e}", s.list),
            )),
            Ok(bytes) => {
                let actual = sha256::hex(&bytes);
                if actual != s.sha256 {
                    out.push(Finding::error(
                        "pin-sha256",
                        format!(
                            "{at}: catalogue/{} has SHA-256 {actual}, the pin says {}",
                            s.list, s.sha256
                        ),
                    ));
                }
                match parse_list(&String::from_utf8_lossy(&bytes)) {
                    Err(e) => out.push(Finding::error(
                        "pin-list",
                        format!("{at}: catalogue/{}: {e}", s.list),
                    )),
                    Ok(rows) => {
                        let mut keys = BTreeSet::new();
                        for r in &rows {
                            if !keys.insert(r.key.clone()) {
                                out.push(Finding::error(
                                    "ref-dup",
                                    format!("catalogue/{}: key `{}` appears twice", s.list, r.key),
                                ));
                            }
                        }
                        lists.insert(s.id.clone(), rows);
                    }
                }
            }
        }
    }
    (out, lists)
}

/// The URL of an upstream file at the pinned commit.
#[must_use]
pub fn raw_url(s: &Source, path: &str) -> String {
    let base = s.raw.clone().unwrap_or_else(|| {
        let repo = s.repo.trim_end_matches('/').trim_end_matches(".git");
        repo.replace("https://github.com/", "https://raw.githubusercontent.com/")
    });
    format!("{}/{}/{}", base.trim_end_matches('/'), s.commit, path)
}

fn download(url: &str, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let status = Command::new("curl")
        .args(["-fsSL", "--proto", "=https,file", "-o"])
        .arg(dest)
        .arg(url)
        .status()
        .map_err(|e| format!("curl: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("curl could not fetch {url} ({status})"))
    }
}

/// Builds a list from an upstream TSV file.
///
/// # Errors
/// When a named column is missing.
pub fn extract_tsv(text: &str, t: &crate::catalogue::TsvExtract) -> Result<String, String> {
    let mut lines = text.lines();
    let header: Vec<&str> = lines
        .next()
        .ok_or("the upstream TSV is empty")?
        .split('\t')
        .collect();
    let col = |name: &str| {
        header
            .iter()
            .position(|h| h.trim() == name)
            .ok_or(format!("the upstream TSV has no column `{name}`"))
    };
    let key = col(&t.key_column)?;
    let title = col(&t.title_column)?;
    let filter = match (&t.where_column, &t.where_value) {
        (Some(c), Some(v)) => Some((col(c)?, v.clone())),
        _ => None,
    };
    let mut out = String::from("key\ttitle\n");
    for line in lines.filter(|l| !l.trim().is_empty()) {
        let cells: Vec<&str> = line.split('\t').collect();
        if let Some((c, v)) = &filter
            && cells.get(*c).map(|s| s.trim()) != Some(v.as_str())
        {
            continue;
        }
        let k = cells.get(key).map_or("", |s| s.trim());
        let tt = cells.get(title).map_or("", |s| s.trim());
        out.push_str(k);
        out.push('\t');
        out.push_str(tt);
        out.push('\n');
    }
    Ok(out)
}

/// `tally refs fetch`: downloads every pinned upstream file of the selected sources into `cache`, checks their
/// SHA-256, and for TSV sources rebuilds the list. With `update`, a rebuilt list replaces the one in the catalogue.
/// Returns the lines to print and whether everything matched.
#[must_use]
pub fn fetch(
    cat: &Catalogue,
    only: Option<&str>,
    cache: &Path,
    update: bool,
) -> (Vec<String>, bool) {
    let mut lines = Vec::new();
    let mut ok = true;
    let sources: Vec<&Source> = cat
        .sources
        .source
        .iter()
        .filter(|s| only.is_none_or(|o| o == s.id))
        .collect();
    if sources.is_empty() {
        return (
            vec![format!(
                "no source {}",
                only.unwrap_or("in catalogue/sources.toml")
            )],
            false,
        );
    }
    for s in sources {
        let dir: PathBuf = cache.join(&s.id).join(&s.commit);
        for (path, pin) in &s.upstream {
            let dest = dir.join(path);
            if let Err(e) = download(&raw_url(s, path), &dest) {
                lines.push(format!("{}: {e}", s.id));
                ok = false;
                continue;
            }
            let actual = fs::read(&dest).map_or_default(|b| sha256::hex(&b));
            if &actual == pin {
                lines.push(format!("{}: {path} matches its pin", s.id));
            } else {
                lines.push(format!(
                    "{}: {path} has SHA-256 {actual}, the pin says {pin}",
                    s.id
                ));
                ok = false;
            }
        }
        if let (Some(t), "tsv") = (&s.tsv, s.extractor.as_str()) {
            let text = fs::read_to_string(dir.join(&t.from)).unwrap_or_default();
            match extract_tsv(&text, t) {
                Ok(list) => {
                    let hash = sha256::hex(list.as_bytes());
                    if hash == s.sha256 {
                        lines.push(format!(
                            "{}: catalogue/{} is what the extractor makes",
                            s.id, s.list
                        ));
                    } else if update {
                        match fs::write(cat.dir.join(&s.list), &list) {
                            Ok(()) => lines.push(format!(
                                "{}: wrote catalogue/{}; set sha256 = \"{hash}\" in sources.toml",
                                s.id, s.list
                            )),
                            Err(e) => {
                                lines.push(format!("{}: {e}", s.id));
                                ok = false;
                            }
                        }
                    } else {
                        lines.push(format!("{}: the extractor makes a list with SHA-256 {hash}; rerun with --update to write it", s.id));
                        ok = false;
                    }
                }
                Err(e) => {
                    lines.push(format!("{}: {e}", s.id));
                    ok = false;
                }
            }
        }
    }
    (lines, ok)
}
