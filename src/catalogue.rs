// SPDX-License-Identifier: GPL-3.0-only
//! The catalogue of one repository: `catalogue/tally.toml` (settings), `catalogue/sources.toml` (pinned reference
//! lists), `catalogue/ref/*.tsv` (the lists) and one TOML file per group in sub-directories.

use crate::ids::tally_ids_gen as idg;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Where a feature comes from: the original program the repository reimplements, or this project.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Original,
    Ours,
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Origin::Original => "original",
            Origin::Ours => "ours",
        })
    }
}

/// What the catalogue says about an item. The states tally reports (tagged, passing, covered, gated) are computed
/// from this and the tests.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
pub enum Declared {
    #[default]
    #[serde(rename = "missing")]
    Missing,
    #[serde(rename = "partial")]
    Partial,
    #[serde(rename = "implemented")]
    Implemented,
    #[serde(rename = "n/a")]
    NotApplicable,
    #[serde(rename = "deferred")]
    Deferred,
}

impl Declared {
    /// True for partial and implemented.
    #[must_use]
    pub fn is_implemented(self) -> bool {
        matches!(self, Declared::Partial | Declared::Implemented)
    }
}

/// `catalogue/tally.toml`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    #[serde(default)]
    pub repo: RepoSettings,
    #[serde(default)]
    pub scan: ScanSettings,
    #[serde(default)]
    pub ids: IdsSettings,
    /// Titles of the areas (the first segment of a group id).
    #[serde(default)]
    pub areas: BTreeMap<String, String>,
    #[serde(default)]
    pub data: Vec<DataCases>,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoSettings {
    /// The cargo package name, which nextest uses as the test binary id (`<package>`, `<package>::<test file>`).
    /// Read from `Cargo.toml` when absent.
    pub package: Option<String>,
    /// A display name for reports; the package name when absent.
    pub name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanSettings {
    /// Directories (relative to the repository root) whose `.rs` files are scanned for `covers(&[...])`.
    #[serde(default = "default_scan_paths")]
    pub paths: Vec<String>,
}

impl Default for ScanSettings {
    fn default() -> Self {
        ScanSettings {
            paths: default_scan_paths(),
        }
    }
}

fn default_scan_paths() -> Vec<String> {
    vec!["src".into(), "tests".into()]
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdsSettings {
    /// The copy of the generator build.rs includes; `tally check` fails when it differs from this tally's.
    #[serde(default = "default_generator")]
    pub generator: Option<String>,
}

impl Default for IdsSettings {
    fn default() -> Self {
        IdsSettings {
            generator: default_generator(),
        }
    }
}

#[allow(clippy::unnecessary_wraps)]
fn default_generator() -> Option<String> {
    Some("catalogue/tally_ids_gen.rs".into())
}

/// Data-driven test cases: each file matching `files` lists `covers = [...]`, run by the test `test`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataCases {
    /// A path pattern relative to the repository root; `*` matches one path segment (`tests/golden/*/covers.toml`).
    pub files: String,
    /// The test that runs these cases: `<file>::<function>`, for example `tests/golden.rs::every_case_passes`.
    pub test: String,
}

/// An importer whose rows say which items are implemented; the catalogue must agree with it.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    /// `tsv`, `ledger` or `capabilities`; `contracts` is planned.
    pub kind: String,
    /// The file, relative to the repository root.
    pub path: String,
    /// The column with the full item id (TSV, default `id`), or the function address (ledger, default `address`).
    pub key_column: Option<String>,
    /// The column with the state. TSV: when absent every row counts as implemented. Ledger: default `status`.
    pub status_column: Option<String>,
    /// The values of `status_column` that mean implemented. Ledger default: `translated`, `static init (table)`.
    #[serde(default)]
    pub implemented: Vec<String>,
    /// Only these groups are compared; all groups when empty. A ledger names exactly one: the group of the functions.
    #[serde(default)]
    pub groups: Vec<String>,
}

/// `catalogue/sources.toml`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sources {
    #[serde(default)]
    pub source: Vec<Source>,
}

/// One pinned reference: an upstream repository at a tag and commit, and the list made from it.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub id: String,
    pub name: Option<String>,
    /// `https://github.com/<owner>/<repo>`.
    pub repo: String,
    /// The release tag the list was read from.
    pub tag: String,
    /// The commit the tag points to (40 hex digits).
    pub commit: String,
    pub licence: String,
    /// The date the upstream files were read (YYYY-MM-DD).
    pub read: String,
    /// How the list is made: `manual` (written by hand from `upstream`), `tsv` (taken from an upstream TSV file),
    /// `ledger` (one row per function of an nv-rs ledger or engine map).
    pub extractor: String,
    /// The list, relative to the catalogue directory (`ref/<name>.tsv`).
    pub list: String,
    /// SHA-256 of the list file.
    pub sha256: String,
    /// SHA-256 of each upstream file at `commit`, by its path in the upstream repository.
    #[serde(default)]
    pub upstream: BTreeMap<String, String>,
    /// For `extractor = "tsv"`: which upstream file and columns.
    pub tsv: Option<TsvExtract>,
    /// For `extractor = "ledger"`: which upstream file.
    pub ledger: Option<LedgerExtract>,
    /// Base URL of the raw files; `https://raw.githubusercontent.com/<owner>/<repo>` when absent.
    pub raw: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TsvExtract {
    /// The upstream path of the TSV file (one of `upstream`).
    pub from: String,
    pub key_column: String,
    pub title_column: String,
    /// Keep only rows whose `where_column` equals `where_value`.
    pub where_column: Option<String>,
    pub where_value: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerExtract {
    /// The upstream path of the ledger or engine map (one of `upstream`), with `address` and `name` columns.
    pub from: String,
}

/// One group file.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupFile {
    pub group: GroupHead,
    #[serde(default)]
    pub item: Vec<ItemDecl>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupHead {
    pub id: String,
    pub title: String,
    pub origin: Origin,
    /// The source (`sources.toml` id) the group follows, if any.
    pub reference: Option<String>,
    /// A reference list (relative to the catalogue directory) whose rows become items.
    pub items_from: Option<String>,
    /// The state of items that come from `items_from` and have no `[[item]]` of their own.
    pub items_from_status: Option<Declared>,
    pub notes: Option<String>,
    /// Column headings for `tally doc` tables.
    pub item_column: Option<String>,
    pub status_column: Option<String>,
    pub notes_column: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemDecl {
    pub id: String,
    pub title: Option<String>,
    #[serde(default)]
    pub status: Declared,
    /// The status text in generated tables, when it says more than the state (`supported, except look-around`).
    pub label: Option<String>,
    pub notes: Option<String>,
    /// Why an item is n/a or deferred.
    pub reason: Option<String>,
    /// Why the item cannot be covered by CI (it needs game data or an oracle).
    pub gated: Option<String>,
    /// Rows of reference lists this item answers: `<source id>:<key>`.
    #[serde(default, rename = "ref")]
    pub refs: Vec<String>,
    /// Where it is implemented: `<path>` or `<path>#<text the file contains>`, relative to the repository root.
    #[serde(default)]
    pub evidence: Vec<String>,
    /// Earlier full ids of this item.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Why the item is retired. Its id stays reserved.
    pub retired: Option<String>,
    pub origin: Option<Origin>,
}

/// An item with its group's context, as the rest of tally sees it.
#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    pub group: String,
    pub title: String,
    pub origin: Origin,
    pub declared: Declared,
    pub label: Option<String>,
    pub notes: Option<String>,
    pub reason: Option<String>,
    pub gated: Option<String>,
    pub refs: Vec<String>,
    pub evidence: Vec<String>,
    pub aliases: Vec<String>,
    pub retired: Option<String>,
    /// The file it is declared in (relative to the repository root, `/` separators).
    pub file: String,
}

/// A group with its items.
#[derive(Clone, Debug)]
pub struct Group {
    pub head: GroupHead,
    pub file: String,
    pub items: Vec<Item>,
}

impl Group {
    #[must_use]
    pub fn area(&self) -> &str {
        self.head.id.split('.').next().unwrap_or("")
    }
}

/// A row of a reference list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefRow {
    pub key: String,
    pub title: String,
}

/// Everything read from one repository.
#[derive(Clone, Debug)]
pub struct Catalogue {
    pub root: PathBuf,
    pub dir: PathBuf,
    pub settings: Settings,
    pub sources: Sources,
    pub groups: Vec<Group>,
    /// Problems found while reading: a file that does not parse is reported here so the other checks still run.
    pub errors: Vec<String>,
}

impl Catalogue {
    /// Every item, retired ones included.
    pub fn items(&self) -> impl Iterator<Item = &Item> {
        self.groups.iter().flat_map(|g| g.items.iter())
    }

    /// The cargo package name.
    #[must_use]
    pub fn package(&self) -> String {
        if let Some(p) = &self.settings.repo.package {
            return p.clone();
        }
        package_from_cargo(&self.root).unwrap_or_else(|| "crate".into())
    }

    /// The display name.
    #[must_use]
    pub fn name(&self) -> String {
        self.settings
            .repo
            .name
            .clone()
            .unwrap_or_else(|| self.package())
    }
}

/// Path relative to `root` with `/` separators.
#[must_use]
pub fn rel(root: &Path, path: &Path) -> String {
    let p = path.strip_prefix(root).unwrap_or(path);
    p.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn package_from_cargo(root: &Path) -> Option<String> {
    let text = fs::read_to_string(root.join("Cargo.toml")).ok()?;
    let value: toml::Table = toml::from_str(&text).ok()?;
    value
        .get("package")?
        .get("name")?
        .as_str()
        .map(str::to_string)
}

/// Reads a TSV reference list: a header line naming the columns (`key` and `title` first), then one row per line.
///
/// # Errors
/// When the file cannot be read or has no header.
pub fn read_list(path: &Path) -> Result<Vec<RefRow>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse_list(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Parses the text of a reference list.
///
/// # Errors
/// When the header is not `key<TAB>title...` or a row has an empty key.
pub fn parse_list(text: &str) -> Result<Vec<RefRow>, String> {
    let mut lines = text.lines();
    let header = lines
        .next()
        .ok_or("empty list: the first line must be `key<TAB>title`")?;
    let cols: Vec<&str> = header.split('\t').collect();
    if cols.len() < 2 || cols[0] != "key" || cols[1] != "title" {
        return Err("the first line must be `key<TAB>title`".into());
    }
    let mut rows = Vec::new();
    for (n, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let key = parts.next().unwrap_or("").trim().to_string();
        let title = parts.next().unwrap_or("").trim().to_string();
        if key.is_empty() {
            return Err(format!("line {}: empty key", n + 2));
        }
        rows.push(RefRow { key, title });
    }
    Ok(rows)
}

fn read_toml<T: serde::de::DeserializeOwned>(
    path: &Path,
    errors: &mut Vec<String>,
    root: &Path,
) -> Option<T> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            errors.push(format!("{}: {e}", rel(root, path)));
            return None;
        }
    };
    match toml::from_str(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            errors.push(format!("{}: {}", rel(root, path), e.to_string().trim_end()));
            None
        }
    }
}

/// Reads the catalogue under `root` (the repository) from `dir` (usually `root/catalogue`).
///
/// # Errors
/// When `dir` does not exist. Problems inside files are collected in [`Catalogue::errors`].
pub fn load(root: &Path, dir: &Path) -> Result<Catalogue, String> {
    if !dir.is_dir() {
        return Err(format!("no catalogue directory at {}", dir.display()));
    }
    let mut errors = Vec::new();
    let settings_path = dir.join("tally.toml");
    let settings = if settings_path.exists() {
        read_toml(&settings_path, &mut errors, root).unwrap_or_default()
    } else {
        Settings::default()
    };
    let sources_path = dir.join("sources.toml");
    let sources = if sources_path.exists() {
        read_toml(&sources_path, &mut errors, root).unwrap_or_default()
    } else {
        Sources::default()
    };

    // Top-level files other than the two settings files are mistakes: groups live in sub-directories.
    if let Ok(rd) = fs::read_dir(dir) {
        let mut names: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
        names.sort();
        for p in names {
            let is_toml = p.extension().is_some_and(|e| e == "toml");
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if p.is_file() && is_toml && name != "tally.toml" && name != "sources.toml" {
                errors.push(format!(
                    "{}: group files belong in a sub-directory named after the area",
                    rel(root, &p)
                ));
            }
        }
    }

    let mut groups = Vec::new();
    for file in idg::group_files(dir)? {
        let Some(gf) = read_toml::<GroupFile>(&file, &mut errors, root) else {
            continue;
        };
        let file_rel = rel(root, &file);
        let mut items: Vec<Item> = gf
            .item
            .iter()
            .map(|d| Item {
                id: format!("{}.{}", gf.group.id, d.id),
                group: gf.group.id.clone(),
                title: d.title.clone().unwrap_or_else(|| d.id.clone()),
                origin: d.origin.unwrap_or(gf.group.origin),
                declared: d.status,
                label: d.label.clone(),
                notes: d.notes.clone(),
                reason: d.reason.clone(),
                gated: d.gated.clone(),
                refs: d.refs.clone(),
                evidence: d.evidence.clone(),
                aliases: d.aliases.clone(),
                retired: d.retired.clone(),
                file: file_rel.clone(),
            })
            .collect();
        if let Some(list) = &gf.group.items_from {
            match read_list(&dir.join(list)) {
                Ok(rows) => {
                    for row in rows {
                        let id = format!("{}.{}", gf.group.id, row.key);
                        if items.iter().any(|i| i.id == id) {
                            continue;
                        }
                        items.push(Item {
                            id,
                            group: gf.group.id.clone(),
                            title: row.title.clone(),
                            origin: gf.group.origin,
                            declared: gf.group.items_from_status.unwrap_or_default(),
                            label: None,
                            notes: None,
                            reason: None,
                            gated: None,
                            refs: gf
                                .group
                                .reference
                                .iter()
                                .map(|s| format!("{s}:{}", row.key))
                                .collect(),
                            evidence: Vec::new(),
                            aliases: Vec::new(),
                            retired: None,
                            file: file_rel.clone(),
                        });
                    }
                }
                Err(e) => errors.push(format!("{file_rel}: items_from: {e}")),
            }
        }
        groups.push(Group {
            head: gf.group,
            file: file_rel,
            items,
        });
    }
    Ok(Catalogue {
        root: root.to_path_buf(),
        dir: dir.to_path_buf(),
        settings,
        sources,
        groups,
        errors,
    })
}
