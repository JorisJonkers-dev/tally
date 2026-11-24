// SPDX-License-Identifier: GPL-3.0-only
//! Importers: files of other tools read as evidence (which items are implemented) or as reference lists.
//!
//! - `tsv`: a plain TSV file with an id column and, optionally, a state column.
//! - `ledger`: the nv-rs function ledger (`scripts/ledger --tsv`) or engine map (`research/engine-map/
//!   engine_map.tsv`): one row per engine function, keyed by its address. The item of a function is
//!   `<group>.f<address>` with the address as eight lower-case hex digits (`engine.functions.f00401000`).

use crate::catalogue::Evidence;

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
/// When the group is not exactly one, a column is missing or an address is not hex.
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
    t.rows
        .iter()
        .map(|r| {
            let key = ledger_key(cell(r, addr))?;
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
