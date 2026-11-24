// SPDX-License-Identifier: GPL-3.0-only
//! `tally doc <file>`: fills the generated tables of a hand-written document. Between
//! `<!-- tally:group <group id> -->` and `<!-- tally:end -->` the text is replaced by the group's items as a
//! Markdown table; everything outside the markers stays as written.

use crate::catalogue::{Catalogue, Declared, Group};
use std::fmt::Write as _;

const BEGIN: &str = "<!-- tally:group ";
const END: &str = "<!-- tally:end -->";

/// The status word of a declared state, for tables.
#[must_use]
pub fn word(d: Declared) -> &'static str {
    match d {
        Declared::Missing => "unsupported",
        Declared::Partial => "partial",
        Declared::Implemented => "supported",
        Declared::NotApplicable => "not applicable",
        Declared::Deferred => "deferred",
    }
}

fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

/// The table of one group.
#[must_use]
pub fn table(g: &Group) -> String {
    let h = &g.head;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "| {} | {} | {} | Id |",
        h.item_column.as_deref().unwrap_or("Feature"),
        h.status_column.as_deref().unwrap_or("Status"),
        h.notes_column.as_deref().unwrap_or("Notes")
    );
    s.push_str("| --- | --- | --- | --- |\n");
    for it in g.items.iter().filter(|i| i.retired.is_none()) {
        let status = it
            .label
            .clone()
            .unwrap_or_else(|| word(it.declared).to_string());
        let notes = it
            .notes
            .clone()
            .or_else(|| it.reason.clone())
            .unwrap_or_default();
        let local = it.id.rsplit('.').next().unwrap_or("");
        let _ = writeln!(
            s,
            "| {} | {} | {} | `{local}` |",
            cell(&it.title),
            cell(&status),
            cell(&notes)
        );
    }
    s
}

/// Fills every marked block of `text`.
///
/// # Errors
/// When a block names an unknown group or is not closed.
pub fn fill(cat: &Catalogue, text: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find(BEGIN) {
        let after = &rest[start + BEGIN.len()..];
        let close = after
            .find("-->")
            .ok_or("a `<!-- tally:group` marker is not closed with `-->`")?;
        let id = after[..close].trim();
        let group = cat.groups.iter().find(|g| g.head.id == id).ok_or(format!(
            "the document names group `{id}`, which the catalogue does not have"
        ))?;
        let body = &after[close + 3..];
        let end = body
            .find(END)
            .ok_or(format!("the block of `{id}` has no `{END}`"))?;
        out.push_str(&rest[..start]);
        let _ = write!(out, "{BEGIN}{id} -->\n{}{END}", table(group));
        rest = &body[end + END.len()..];
    }
    out.push_str(rest);
    Ok(out)
}
