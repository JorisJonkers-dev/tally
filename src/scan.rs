// SPDX-License-Identifier: GPL-3.0-only
//! Finds the `covers(&[ids::...])` calls in Rust sources and the `covers = [...]` lists of data-driven cases.
//!
//! The scanner is a small tokenizer, not a parser: it skips comments, strings and character literals, follows
//! braces to know which function a call is in, and reads the attributes in front of each function (`#[test]`,
//! `#[ignore]`). That is enough because the ids are typed constants: the compiler already proves that every
//! `ids::...` path exists, and tally only needs to know which test names which id.

use crate::catalogue::{Catalogue, rel};
use std::fs;
use std::path::{Path, PathBuf};

/// Where a test lives: the file (relative to the repository root) and the function name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TestRef {
    pub file: String,
    pub function: String,
}

impl std::fmt::Display for TestRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}", self.file, self.function)
    }
}

/// One `covers(...)` call, or one data case file.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Site {
    pub file: String,
    pub line: usize,
    /// The test it belongs to; `None` when the call is outside any function.
    pub test: Option<TestRef>,
    /// The enclosing function carries a test attribute (always true for data cases).
    pub is_test: bool,
    /// The test is `#[ignore]`d: it never runs, so it never counts.
    pub ignored: bool,
    /// The ids named, dotted (`area.group.item`).
    pub ids: Vec<String>,
    /// Arguments that are not a full `ids::...` constant path.
    pub bad: Vec<String>,
    /// A `// covers-table: <reason>` comment justifies more than ten ids.
    pub table: bool,
    /// True for a data case file and for a row of a covers map.
    pub data: bool,
    /// True for a row of a covers map (`[[map]]`): a test in a file the repository does not tag in place.
    pub map: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Punct(char),
}

struct Lexed {
    toks: Vec<(Tok, usize)>,
    comments: Vec<(usize, String)>,
}

fn lex(src: &str) -> Lexed {
    let chars: Vec<char> = src.chars().collect();
    let mut toks = Vec::new();
    let mut comments = Vec::new();
    let mut i = 0;
    let mut line = 1;
    let n = chars.len();
    let count_lines = |s: &[char]| s.iter().filter(|c| **c == '\n').count();
    while i < n {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '/' && i + 1 < n && chars[i + 1] == '/' {
            let start = i;
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            comments.push((line, chars[start..i].iter().collect()));
        } else if c == '/' && i + 1 < n && chars[i + 1] == '*' {
            let start = i;
            let start_line = line;
            let mut depth = 0;
            while i < n {
                if chars[i] == '/' && i + 1 < n && chars[i + 1] == '*' {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && i + 1 < n && chars[i + 1] == '/' {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            let text: String = chars[start..i.min(n)].iter().collect();
            line += count_lines(&chars[start..i.min(n)]);
            comments.push((start_line, text));
        } else if c == '"' || ((c == 'b' || c == 'c') && i + 1 < n && chars[i + 1] == '"') {
            if c != '"' {
                i += 1;
            }
            i += 1;
            while i < n && chars[i] != '"' {
                if chars[i] == '\\' {
                    i += 1;
                }
                if i < n && chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            i += 1;
        } else if (c == 'r' || ((c == 'b' || c == 'c') && i + 1 < n && chars[i + 1] == 'r')) && {
            let mut j = i + if c == 'r' { 1 } else { 2 };
            while j < n && chars[j] == '#' {
                j += 1;
            }
            j < n && chars[j] == '"'
        } {
            let mut j = i + if c == 'r' { 1 } else { 2 };
            let mut hashes = 0;
            while chars[j] == '#' {
                hashes += 1;
                j += 1;
            }
            j += 1;
            let start = j;
            loop {
                if j >= n {
                    break;
                }
                if chars[j] == '"' && (0..hashes).all(|k| j + 1 + k < n && chars[j + 1 + k] == '#')
                {
                    break;
                }
                j += 1;
            }
            line += count_lines(&chars[start..j.min(n)]);
            i = (j + 1 + hashes).min(n);
        } else if c == '\'' {
            // A character literal ('a', '\n', '\u{..}') or a lifetime ('a).
            if i + 2 < n && chars[i + 1] == '\\' {
                i += 2;
                while i < n && chars[i] != '\'' {
                    i += 1;
                }
                i += 1;
            } else if i + 2 < n && chars[i + 2] == '\'' {
                i += 3;
            } else {
                i += 1;
            }
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            // Raw identifiers: r#name.
            if word == "r"
                && i + 1 < n
                && chars[i] == '#'
                && (chars[i + 1].is_alphabetic() || chars[i + 1] == '_')
            {
                i += 1;
                let s = i;
                while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                toks.push((Tok::Ident(chars[s..i].iter().collect()), line));
            } else {
                toks.push((Tok::Ident(word), line));
            }
        } else if c.is_ascii_digit() {
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.') {
                if chars[i] == '.' && i + 1 < n && chars[i + 1] == '.' {
                    break;
                }
                i += 1;
            }
        } else {
            toks.push((Tok::Punct(c), line));
            i += 1;
        }
    }
    Lexed { toks, comments }
}

struct Frame {
    name: String,
    is_test: bool,
    ignored: bool,
    depth: usize,
    line: usize,
}

struct Attr {
    is_test: bool,
    ignored: bool,
}

fn is_ident(t: Option<&(Tok, usize)>, word: &str) -> bool {
    matches!(t, Some((Tok::Ident(w), _)) if w == word)
}

fn is_punct(t: Option<&(Tok, usize)>, c: char) -> bool {
    matches!(t, Some((Tok::Punct(p), _)) if *p == c)
}

/// Reads `#[...]` starting at `i` (the `#`); returns the end index (after `]`) and what it says.
fn read_attr(toks: &[(Tok, usize)], i: usize) -> (usize, Attr) {
    let mut j = i + 1;
    if is_punct(toks.get(j), '!') {
        j += 1;
    }
    j += 1; // '['
    let mut depth = 1;
    let mut first_path: Vec<String> = Vec::new();
    let mut in_first_path = true;
    let mut words: Vec<String> = Vec::new();
    while j < toks.len() && depth > 0 {
        match &toks[j].0 {
            Tok::Punct('[' | '(') => {
                depth += 1;
                in_first_path = false;
            }
            Tok::Punct(']' | ')') => depth -= 1,
            Tok::Punct(':') => {}
            Tok::Punct(_) => in_first_path = false,
            Tok::Ident(w) => {
                if in_first_path {
                    first_path.push(w.clone());
                }
                words.push(w.clone());
            }
        }
        j += 1;
    }
    let last = first_path.last().map_or("", String::as_str);
    let is_test =
        last == "test" || last.ends_with("_test") || last == "rstest" || last == "test_case";
    let ignored = words.first().is_some_and(|w| w == "ignore")
        || (words.first().is_some_and(|w| w == "cfg_attr") && words.iter().any(|w| w == "ignore"));
    (j, Attr { is_test, ignored })
}

/// Reads the argument list of `covers(&[ ... ])` starting at the `[`; returns the end index, the ids and the bad
/// arguments.
fn read_ids(toks: &[(Tok, usize)], start: usize) -> (usize, Vec<String>, Vec<String>) {
    let mut j = start + 1;
    let mut ids = Vec::new();
    let mut bad = Vec::new();
    let mut path: Vec<String> = Vec::new();
    let mut junk = false;
    let finish =
        |path: &mut Vec<String>, junk: &mut bool, ids: &mut Vec<String>, bad: &mut Vec<String>| {
            if path.is_empty() && !*junk {
                return;
            }
            let text = path.join("::");
            match path.iter().position(|s| s == "ids") {
                Some(pos) if !*junk && path.len() - pos >= 4 => {
                    let segs = &path[pos + 1..];
                    let last = &segs[segs.len() - 1];
                    let upper = last
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
                    let mods_lower = segs[..segs.len() - 1]
                        .iter()
                        .all(|s| s.chars().all(|c| !c.is_ascii_uppercase()));
                    if upper && mods_lower {
                        let mut id: Vec<String> = segs[..segs.len() - 1].to_vec();
                        id.push(last.to_ascii_lowercase());
                        ids.push(id.join("."));
                    } else {
                        bad.push(text);
                    }
                }
                _ => bad.push(if text.is_empty() {
                    "<expression>".into()
                } else {
                    text
                }),
            }
            path.clear();
            *junk = false;
        };
    let mut depth = 0;
    while j < toks.len() {
        match &toks[j].0 {
            Tok::Punct(']') if depth == 0 => {
                finish(&mut path, &mut junk, &mut ids, &mut bad);
                return (j + 1, ids, bad);
            }
            Tok::Punct(',') if depth == 0 => finish(&mut path, &mut junk, &mut ids, &mut bad),
            Tok::Punct(':') => {}
            Tok::Punct('(' | '[' | '{') => {
                depth += 1;
                junk = true;
            }
            Tok::Punct(')' | ']' | '}') => {
                depth -= 1;
                junk = true;
            }
            Tok::Ident(w) => {
                if depth == 0 {
                    path.push(w.clone());
                }
            }
            Tok::Punct(_) => junk = true,
        }
        j += 1;
    }
    (j, ids, bad)
}

/// A test function found in the sources.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestFn {
    pub test: TestRef,
    pub ignored: bool,
}

/// What a scan found.
#[derive(Clone, Debug, Default)]
pub struct Scan {
    pub sites: Vec<Site>,
    pub tests: Vec<TestFn>,
    pub errors: Vec<String>,
}

/// Scans one Rust source: the `covers` calls and the test functions.
#[must_use]
pub fn scan_source(file: &str, src: &str) -> (Vec<Site>, Vec<TestFn>) {
    let Lexed { toks, comments } = lex(src);
    let mut sites = Vec::new();
    let mut tests = Vec::new();
    let mut frames: Vec<Frame> = Vec::new();
    let mut attrs: Vec<Attr> = Vec::new();
    let mut pending: Option<Frame> = None;
    let mut depth = 0usize;
    let mut i = 0;
    while i < toks.len() {
        match &toks[i].0 {
            Tok::Punct('#')
                if is_punct(toks.get(i + 1), '[')
                    || (is_punct(toks.get(i + 1), '!') && is_punct(toks.get(i + 2), '[')) =>
            {
                let (end, attr) = read_attr(&toks, i);
                attrs.push(attr);
                i = end;
                continue;
            }
            Tok::Ident(w) if w == "fn" => {
                if let Some((Tok::Ident(name), line)) = toks.get(i + 1) {
                    pending = Some(Frame {
                        name: name.clone(),
                        is_test: attrs.iter().any(|a| a.is_test),
                        ignored: attrs.iter().any(|a| a.ignored),
                        depth: 0,
                        line: *line,
                    });
                    attrs.clear();
                    i += 2;
                    continue;
                }
            }
            Tok::Ident(w)
                if w == "covers" && !is_ident(i.checked_sub(1).and_then(|k| toks.get(k)), "fn") =>
            {
                if is_punct(toks.get(i + 1), '(')
                    && is_punct(toks.get(i + 2), '&')
                    && is_punct(toks.get(i + 3), '[')
                {
                    let line = toks[i].1;
                    let (end, ids, bad) = read_ids(&toks, i + 3);
                    let frame = frames.last();
                    let from = frame.map_or(line, |f| f.line.saturating_sub(3));
                    let table = comments
                        .iter()
                        .any(|(l, t)| *l >= from && *l <= line && t.contains("covers-table:"));
                    let bad = if ids.is_empty() && bad.is_empty() {
                        vec!["<empty list>".to_string()]
                    } else {
                        bad
                    };
                    sites.push(Site {
                        file: file.to_string(),
                        line,
                        test: frame.map(|f| TestRef {
                            file: file.to_string(),
                            function: f.name.clone(),
                        }),
                        is_test: frame.is_some_and(|f| f.is_test),
                        ignored: frame.is_some_and(|f| f.ignored),
                        ids,
                        bad,
                        table,
                        data: false,
                        map: false,
                    });
                    i = end;
                    continue;
                }
            }
            Tok::Punct('{') => {
                depth += 1;
                if let Some(mut f) = pending.take() {
                    f.depth = depth;
                    if f.is_test {
                        tests.push(TestFn {
                            test: TestRef {
                                file: file.to_string(),
                                function: f.name.clone(),
                            },
                            ignored: f.ignored,
                        });
                    }
                    frames.push(f);
                }
                attrs.clear();
            }
            Tok::Punct('}') => {
                if frames.last().is_some_and(|f| f.depth == depth) {
                    frames.pop();
                }
                depth = depth.saturating_sub(1);
                attrs.clear();
            }
            Tok::Punct(';') => {
                pending = None;
                attrs.clear();
            }
            _ => {}
        }
        i += 1;
    }
    (sites, tests)
}

/// The files that `pattern` (relative to `root`, `/` separated, `*` for one whole or partial segment) matches.
#[must_use]
pub fn glob(root: &Path, pattern: &str) -> Vec<PathBuf> {
    let mut current = vec![root.to_path_buf()];
    for seg in pattern.split('/').filter(|s| !s.is_empty()) {
        let mut next = Vec::new();
        for dir in &current {
            if seg.contains('*') {
                let Ok(rd) = fs::read_dir(dir) else { continue };
                let mut names: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
                names.sort();
                for p in names {
                    if p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| wildcard(seg, n))
                    {
                        next.push(p);
                    }
                }
            } else {
                let p = dir.join(seg);
                if p.exists() {
                    next.push(p);
                }
            }
        }
        current = next;
    }
    current.into_iter().filter(|p| p.is_file()).collect()
}

fn wildcard(pattern: &str, name: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == name,
        Some((pre, rest)) => {
            let Some(tail) = name.strip_prefix(pre) else {
                return false;
            };
            (0..=tail.len())
                .filter(|k| tail.is_char_boundary(*k))
                .any(|k| wildcard(rest, &tail[k..]))
        }
    }
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut names: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    names.sort();
    for p in names {
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

fn escapes_root(path: &str) -> bool {
    path.starts_with('/')
        || path.split('/').any(|seg| seg == "..")
        || path.contains(':')
        || path.contains('\\')
}

/// A path written the one way tally writes it: `/` separators, no empty, `.` or `..` segment. Two spellings of one
/// file would be two tests to the ten-id guard.
fn canonical(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && path
            .split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

/// The rows of a covers map: a TSV file with the header `test<TAB>covers` (and an optional third column `table`,
/// the reason a row may name more than ten ids). `test` is `<file>::<function>` with the file relative to the
/// repository root; `covers` lists full dotted ids, separated by commas or spaces. Empty lines and lines starting
/// with `#` are skipped. A map tags tests in files the repository does not edit (nv-rs: upstream's test files).
///
/// # Errors
/// When the header is wrong or a row has no test, a test without `::` or no ids.
pub fn map_rows(file: &str, text: &str) -> Result<Vec<Site>, String> {
    let mut lines = text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.starts_with('#'));
    let Some((_, header)) = lines.next() else {
        return Err(format!("{file}: no `test<TAB>covers` header"));
    };
    let cols: Vec<&str> = header.split('\t').map(str::trim).collect();
    if cols.len() < 2
        || cols[0] != "test"
        || cols[1] != "covers"
        || cols.get(2).is_some_and(|c| *c != "table")
        || cols.len() > 3
    {
        return Err(format!(
            "{file}: the header must be `test<TAB>covers` or `test<TAB>covers<TAB>table`, not `{header}`"
        ));
    }
    let mut out = Vec::new();
    for (n, line) in lines {
        let line_no = n + 1;
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() > cols.len() {
            return Err(format!(
                "{file}:{line_no}: {} fields, but the header has {}",
                fields.len(),
                cols.len()
            ));
        }
        let named = fields[0].trim();
        let Some((test_file, function)) = named.rsplit_once("::") else {
            return Err(format!(
                "{file}:{line_no}: the test `{named}` must be `<file>::<function>`"
            ));
        };
        if !canonical(test_file) {
            return Err(format!(
                "{file}:{line_no}: write the file `{test_file}` relative to the repository root, with `/` and no `.`, `..` or empty segment"
            ));
        }
        let ids: Vec<String> = fields
            .get(1)
            .copied()
            .unwrap_or("")
            .split([',', ' '])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        if ids.is_empty() {
            return Err(format!("{file}:{line_no}: `{named}` covers no ids"));
        }
        let table = fields.get(2).is_some_and(|r| !r.trim().is_empty());
        out.push(Site {
            file: file.to_string(),
            line: line_no,
            test: Some(TestRef {
                file: test_file.to_string(),
                function: function.to_string(),
            }),
            is_test: true,
            ignored: false,
            ids,
            bad: Vec::new(),
            table,
            data: true,
            map: true,
        });
    }
    Ok(out)
}

/// Everything the repository tags: `covers` calls in the scan paths and the data case files. Problems with data
/// case files (unreadable, no `covers` list, a bad test reference) are in [`Scan::errors`].
#[must_use]
pub fn scan(cat: &Catalogue) -> Scan {
    let mut sites = Vec::new();
    let mut tests = Vec::new();
    let mut errors = Vec::new();
    let generator = cat.settings.ids.generator.clone().unwrap_or_default();
    for path in &cat.settings.scan.paths {
        let mut files = Vec::new();
        rust_files(&cat.root.join(path), &mut files);
        for f in files {
            let file = rel(&cat.root, &f);
            if file == generator {
                continue;
            }
            match fs::read_to_string(&f) {
                Ok(src) => {
                    let (s, t) = scan_source(&file, &src);
                    sites.extend(s);
                    tests.extend(t);
                }
                Err(e) => errors.push(format!("{file}: {e}")),
            }
        }
    }
    for data in &cat.settings.data {
        let Some((test_file, function)) = data.test.rsplit_once("::") else {
            errors.push(format!(
                "catalogue/tally.toml: data test `{}` must be `<file>::<function>`",
                data.test
            ));
            continue;
        };
        let files = glob(&cat.root, &data.files);
        if files.is_empty() {
            errors.push(format!(
                "catalogue/tally.toml: data files `{}` match nothing",
                data.files
            ));
        }
        for f in files {
            let file = rel(&cat.root, &f);
            let parsed = fs::read_to_string(&f)
                .map_err(|e| e.to_string())
                .and_then(|t| {
                    toml::from_str::<toml::Table>(&t)
                        .map_err(|e| e.to_string().trim_end().to_string())
                });
            let table = match parsed {
                Ok(t) => t,
                Err(e) => {
                    errors.push(format!("{file}: {e}"));
                    continue;
                }
            };
            let Some(list) = table.get("covers").and_then(toml::Value::as_array) else {
                errors.push(format!("{file}: no `covers = [...]` list"));
                continue;
            };
            let mut ids = Vec::new();
            let mut bad = Vec::new();
            for v in list {
                match v.as_str() {
                    Some(s) => ids.push(s.to_string()),
                    None => bad.push(format!("{v:?}")),
                }
            }
            let table_reason = table
                .get("covers_table")
                .and_then(toml::Value::as_str)
                .is_some_and(|s| !s.trim().is_empty());
            sites.push(Site {
                file,
                line: 1,
                test: Some(TestRef {
                    file: test_file.to_string(),
                    function: function.to_string(),
                }),
                is_test: true,
                ignored: false,
                ids,
                bad,
                table: table_reason,
                data: true,
                map: false,
            });
        }
    }
    let scanned: std::collections::BTreeSet<String> =
        tests.iter().map(|t| t.test.file.clone()).collect();
    let mut extra_files: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for map in &cat.settings.map {
        let files = glob(&cat.root, &map.files);
        if files.is_empty() {
            errors.push(format!(
                "catalogue/tally.toml: map files `{}` match nothing",
                map.files
            ));
        }
        for f in files {
            let file = rel(&cat.root, &f);
            match fs::read_to_string(&f) {
                Ok(text) => match map_rows(&file, &text) {
                    Ok(rows) => {
                        for site in rows {
                            if let Some(t) = &site.test {
                                extra_files.insert(t.file.clone());
                            }
                            sites.push(site);
                        }
                    }
                    Err(e) => errors.push(e),
                },
                Err(e) => errors.push(format!("{file}: {e}")),
            }
        }
    }
    // The files a map names are read for their test functions (whether the test exists and is ignored), also when
    // they are outside the scan paths; their own covers() calls are not taken from there.
    for file in extra_files.difference(&scanned) {
        if escapes_root(file) {
            errors.push(format!(
                "{file}: a covers map names a file outside the repository"
            ));
            continue;
        }
        if let Ok(src) = fs::read_to_string(cat.root.join(file)) {
            let (_, t) = scan_source(file, &src);
            tests.extend(t);
        }
    }
    let ignored: std::collections::BTreeMap<TestRef, bool> =
        tests.iter().map(|t| (t.test.clone(), t.ignored)).collect();
    for s in sites.iter_mut().filter(|s| s.map) {
        if let Some(t) = &s.test {
            s.ignored = ignored.get(t).copied().unwrap_or(false);
        }
    }
    Scan {
        sites,
        tests,
        errors,
    }
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;
