// SPDX-License-Identifier: GPL-3.0-only
//! Importers: files of other tools read as evidence (which items are implemented) or as reference lists.
//!
//! - `tsv`: a plain TSV file with an id column and, optionally, a state column.
//! - `ledger`: the nv-rs function ledger (`scripts/ledger --tsv`) or engine map (`research/engine-map/
//!   engine_map.tsv`): one row per engine function, keyed by its address. The item of a function is
//!   `<group>.f<address>` with the address as eight lower-case hex digits (`engine.functions.f00401000`).
//! - `capabilities`: nv-rs `.fork/capabilities.json`, the script commands each built-in extender implements. The
//!   commands of extender `jip-ln` are the items of the group whose last segment is `jip_ln`; a command's item is its
//!   name as an id segment ([`segment`]: `GetLNVersion` is `getlnversion`).

use crate::catalogue::Evidence;
use crate::ids::tally_ids_gen::valid_segment;
use std::collections::BTreeMap;

/// The states of the nv-rs ledger that mean a function is done: translated by hand, or run from a generated table.
pub const LEDGER_DONE: [&str; 2] = ["translated", "static init (table)"];

/// What an evidence file says: each row's full item id and whether it is implemented.
pub type Rows = Vec<(String, bool)>;

/// The item segment of an engine function: `f` and the address as eight lower-case hex digits.
///
/// # Errors
/// When `addr` is not a hex number of at most eight digits (an optional `0x` is allowed).
pub fn ledger_key(addr: &str) -> Result<String, String> {
    let hex = addr
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    if hex.is_empty() || hex.len() > 8 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "`{addr}` is not an address of at most eight hex digits"
        ));
    }
    let n = u32::from_str_radix(hex, 16).map_err(|e| format!("`{addr}`: {e}"))?;
    Ok(format!("f{n:08x}"))
}

/// `name` as an id segment: lower-case ASCII, every other character `_`, `x` in front when it does not start with a
/// letter, and `_` after a Rust keyword (`Return` is `return_`).
#[must_use]
pub fn segment(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    if !s.starts_with(|c: char| c.is_ascii_lowercase()) {
        s.insert(0, 'x');
    }
    if !valid_segment(&s) {
        s.push('_');
    }
    s
}

/// Item ids for `names` under `group`. A name listed again counts once; two different names that give the same
/// segment are an error.
fn named_rows<'a>(
    group: &str,
    names: impl IntoIterator<Item = &'a str>,
    what: &str,
) -> Result<Rows, String> {
    let mut seen: BTreeMap<String, &str> = BTreeMap::new();
    let mut rows = Vec::new();
    for name in names {
        if name.trim().is_empty() {
            return Err(format!("{what} with an empty name"));
        }
        let seg = segment(name);
        if let Some(other) = seen.insert(seg.clone(), name) {
            if other == name {
                continue;
            }
            return Err(format!(
                "{what} `{other}` and `{name}` both become the item `{group}.{seg}`"
            ));
        }
        rows.push((format!("{group}.{seg}"), true));
    }
    Ok(rows)
}

/// The rows of nv-rs `.fork/capabilities.json`: for each extender whose id (as a segment) is the last segment of one
/// of the evidence's `groups`, every name in `commands.implemented_names` is an implemented item of that group.
/// Extenders without a group are left out; a group without an extender is an error.
///
/// # Errors
/// When the file is not that JSON, `groups` is empty, a group matches no extender, the count of implemented
/// commands disagrees with the list, or two commands give the same item.
pub fn capabilities_rows(text: &str, ev: &Evidence) -> Result<Rows, String> {
    if ev.groups.is_empty() {
        return Err(
            "a capabilities importer names its groups in `groups` (one per extender)".into(),
        );
    }
    let doc: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
    let extenders = doc
        .get("extenders")
        .and_then(serde_json::Value::as_array)
        .ok_or("no `extenders` list")?;
    let mut rows = Vec::new();
    let mut matched = Vec::new();
    for e in extenders {
        let id = e
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or("an extender without an `id`")?;
        let Some(group) = ev
            .groups
            .iter()
            .find(|g| g.rsplit('.').next() == Some(segment(id).as_str()))
        else {
            continue;
        };
        if matched.contains(&group.as_str()) {
            return Err(format!(
                "extender `{id}` is the second extender of group `{group}` (an id listed twice, or two ids that give one segment)"
            ));
        }
        matched.push(group.as_str());
        let commands = e.get("commands");
        let names: Vec<&str> = commands
            .and_then(|c| c.get("implemented_names"))
            .and_then(serde_json::Value::as_array)
            .ok_or(format!(
                "extender `{id}` has no `commands.implemented_names` list"
            ))?
            .iter()
            .map(|n| {
                n.as_str()
                    .ok_or(format!("extender `{id}`: a command name is not a string"))
            })
            .collect::<Result<_, _>>()?;
        if let Some(count) = commands
            .and_then(|c| c.get("implemented"))
            .and_then(serde_json::Value::as_u64)
            && usize::try_from(count).ok() != Some(names.len())
        {
            return Err(format!(
                "extender `{id}` says {count} commands are implemented but lists {}",
                names.len()
            ));
        }
        rows.extend(named_rows(
            group,
            names,
            &format!("extender `{id}`: command"),
        )?);
    }
    if let Some(g) = ev.groups.iter().find(|g| !matched.contains(&g.as_str())) {
        return Err(format!(
            "group `{g}` matches no extender of the file (its last segment must be an extender id)"
        ));
    }
    Ok(rows)
}

/// A TSV table with its header: lines starting with `#` and empty lines are skipped, the first other line is the
/// header.
struct Table<'a> {
    header: Vec<&'a str>,
    rows: Vec<Vec<&'a str>>,
}

impl<'a> Table<'a> {
    fn read(text: &'a str) -> Result<Table<'a>, String> {
        let mut lines = text
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'));
        let header = lines
            .next()
            .ok_or("the file has no header line")?
            .split('\t')
            .map(str::trim)
            .collect();
        let rows = lines
            .map(|l| l.split('\t').map(str::trim).collect())
            .collect();
        Ok(Table { header, rows })
    }

    fn column(&self, name: &str) -> Result<usize, String> {
        self.header
            .iter()
            .position(|h| *h == name)
            .ok_or(format!("no column `{name}`"))
    }
}

fn cell<'a>(row: &[&'a str], col: usize) -> &'a str {
    row.get(col).copied().unwrap_or("")
}

/// The rows of a plain TSV evidence file: `key_column` (default `id`) holds the full item id; with `status_column`,
/// a row is implemented when its state is one of `implemented`, else every row is.
///
/// # Errors
/// When a named column is missing.
pub fn tsv_rows(text: &str, ev: &Evidence) -> Result<Rows, String> {
    let t = Table::read(text)?;
    let key = t.column(ev.key_column.as_deref().unwrap_or("id"))?;
    let status = ev
        .status_column
        .as_deref()
        .map(|c| t.column(c))
        .transpose()?;
    Ok(t.rows
        .iter()
        .map(|r| {
            let done = status.is_none_or(|c| ev.implemented.iter().any(|v| v == cell(r, c)));
            (cell(r, key).to_string(), done)
        })
        .collect())
}

/// The rows of an nv-rs ledger: one per function, `<group>.f<address>`, implemented when its `status` (or
/// `status_column`) is one of `implemented` (default: translated, static init (table)). The address is read from
/// `key_column` (default `address`). The evidence names exactly one group.
///
/// # Errors
/// When the group is not exactly one, a column is missing, an address is not hex or one is listed twice.
pub fn ledger_rows(text: &str, ev: &Evidence) -> Result<Rows, String> {
    let [group] = ev.groups.as_slice() else {
        return Err(
            "a ledger importer names exactly one group in `groups` (the group of the functions)"
                .into(),
        );
    };
    let t = Table::read(text)?;
    let addr = t.column(ev.key_column.as_deref().unwrap_or("address"))?;
    let status = t.column(ev.status_column.as_deref().unwrap_or("status"))?;
    let done: Vec<&str> = if ev.implemented.is_empty() {
        LEDGER_DONE.to_vec()
    } else {
        ev.implemented.iter().map(String::as_str).collect()
    };
    let mut seen = std::collections::BTreeSet::new();
    t.rows
        .iter()
        .map(|r| {
            let key = ledger_key(cell(r, addr))?;
            if !seen.insert(key.clone()) {
                return Err(format!("address {} is listed twice", &key[1..]));
            }
            Ok((format!("{group}.{key}"), done.contains(&cell(r, status))))
        })
        .collect()
}

/// A reference list (`key<TAB>title`) from a ledger or engine map: one row per function, key `f<address>`, title the
/// function's name, or its address when it has none. Rows are sorted by address; an address listed twice is an
/// error.
///
/// # Errors
/// When the `address` or `name` column is missing, an address is not hex, or one is listed twice.
pub fn ledger_list(text: &str) -> Result<String, String> {
    let t = Table::read(text)?;
    let addr = t.column("address")?;
    let name = t.column("name")?;
    let mut rows = std::collections::BTreeMap::new();
    for r in &t.rows {
        let key = ledger_key(cell(r, addr))?;
        let title = match cell(r, name) {
            "" => format!("0x{}", &key[1..]),
            n => n.to_string(),
        };
        if rows.insert(key.clone(), title).is_some() {
            return Err(format!("address {} is listed twice", &key[1..]));
        }
    }
    let mut out = String::from("key\ttitle\n");
    for (k, t) in rows {
        out.push_str(&k);
        out.push('\t');
        out.push_str(&t);
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "import_caps_tests.rs"]
mod caps_tests;
