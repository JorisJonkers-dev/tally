// SPDX-License-Identifier: GPL-3.0-only
//! Test results: `JUnit` XML from cargo-nextest (`[profile.ci.junit]` with `report-skipped = "all"`), read into a
//! small JSON file (`tally-results.v1`) that `check` and `report` read.
//!
//! A case passes only when it ran and has no failure, error, skip, flaky or rerun record: an ignored or skipped
//! test never counts, and neither does a test that needed a retry.

use crate::scan::TestRef;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// The three platforms whose passes make an item covered.
pub const PLATFORMS: [&str; 3] = ["linux", "windows", "macos"];

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Pass,
    Fail,
    Skip,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Case {
    /// The test binary (`<package>`, `<package>::<test file>`).
    pub suite: String,
    /// The test path inside the binary (`module::function`).
    pub name: String,
    pub outcome: Outcome,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Run {
    pub os: String,
    pub cases: Vec<Case>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Results {
    pub schema: String,
    pub runs: Vec<Run>,
}

impl Default for Results {
    fn default() -> Self {
        Results {
            schema: SCHEMA.into(),
            runs: Vec::new(),
        }
    }
}

pub const SCHEMA: &str = "tally-results.v1";

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn attrs(tag: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut rest = tag;
    while let Some(eq) = rest.find('=') {
        let key = rest[..eq]
            .trim()
            .rsplit(char::is_whitespace)
            .next()
            .unwrap_or("")
            .to_string();
        let after = rest[eq + 1..].trim_start();
        let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            break;
        };
        let body = &after[1..];
        let Some(end) = body.find(q) else { break };
        out.insert(key, unescape(&body[..end]));
        rest = &body[end + 1..];
    }
    out
}

/// Reads the cases of a `JUnit` XML report.
///
/// # Errors
/// When the text has no `<testsuites>` or `<testsuite>` element or a case has no name.
pub fn parse_junit(xml: &str) -> Result<Vec<Case>, String> {
    let mut cases = Vec::new();
    let mut suite = String::new();
    let mut seen_suite = false;
    // The case being read: (suite, name, bad).
    let mut open: Option<(String, String, bool)> = None;
    let mut rest = xml;
    while let Some(lt) = rest.find('<') {
        rest = &rest[lt..];
        if let Some(r) = rest.strip_prefix("<!--") {
            rest = r.find("-->").map_or("", |e| &r[e + 3..]);
            continue;
        }
        if let Some(r) = rest.strip_prefix("<![CDATA[") {
            rest = r.find("]]>").map_or("", |e| &r[e + 3..]);
            continue;
        }
        let Some(gt) = rest.find('>') else {
            return Err("an element is not closed".into());
        };
        let tag = &rest[1..gt];
        rest = &rest[gt + 1..];
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        let closing = tag.starts_with('/');
        let self_closing = tag.ends_with('/');
        let body = tag.trim_start_matches('/').trim_end_matches('/');
        let name = body.split_whitespace().next().unwrap_or("");
        match (name, closing) {
            ("testsuite", false) => {
                seen_suite = true;
                suite = attrs(body).get("name").cloned().unwrap_or_default();
            }
            ("testsuites", false) => seen_suite = true,
            ("testcase", false) => {
                let a = attrs(body);
                let case_name = a.get("name").cloned().ok_or("a <testcase> has no name")?;
                let case_suite = a
                    .get("classname")
                    .cloned()
                    .filter(|_| suite.is_empty())
                    .unwrap_or_else(|| suite.clone());
                if self_closing {
                    cases.push(Case {
                        suite: case_suite,
                        name: case_name,
                        outcome: Outcome::Pass,
                    });
                } else {
                    open = Some((case_suite, case_name, false));
                }
            }
            ("testcase", true) => {
                if let Some((s, n, bad)) = open.take() {
                    cases.push(Case {
                        suite: s,
                        name: n,
                        outcome: if bad { Outcome::Fail } else { Outcome::Pass },
                    });
                }
            }
            ("skipped", false) => {
                if let Some((s, n, _)) = open.take() {
                    cases.push(Case {
                        suite: s,
                        name: n,
                        outcome: Outcome::Skip,
                    });
                    // The closing </testcase> then finds nothing open.
                }
            }
            (
                "failure" | "error" | "flakyFailure" | "flakyError" | "rerunFailure" | "rerunError",
                false,
            ) => {
                if let Some(o) = open.as_mut() {
                    o.2 = true;
                }
            }
            _ => {}
        }
    }
    if !seen_suite {
        return Err("not a JUnit report: no <testsuites> or <testsuite> element".into());
    }
    Ok(cases)
}

/// Reads a results file; an absent file is no results.
///
/// # Errors
/// When the file exists but is not a `tally-results.v1` document.
pub fn load(path: &Path) -> Result<Results, String> {
    if !path.exists() {
        return Err(format!("{}: no such file", path.display()));
    }
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let r: Results = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if r.schema != SCHEMA {
        return Err(format!(
            "{}: schema `{}`, expected `{SCHEMA}`",
            path.display(),
            r.schema
        ));
    }
    Ok(r)
}

/// The OS a report file is for, from its name, when `--os` is not given.
#[must_use]
pub fn os_from_name(path: &str) -> Option<&'static str> {
    let name = Path::new(path)
        .file_name()
        .map_or(path.into(), |n| n.to_string_lossy());
    let lower = name.to_ascii_lowercase();
    let named: Vec<&'static str> = PLATFORMS
        .iter()
        .copied()
        .filter(|p| lower.contains(p))
        .collect();
    if named.len() > 1 {
        return None;
    }
    named.first().copied().or_else(|| {
        if lower.contains("ubuntu") {
            Some("linux")
        } else if lower.contains("darwin") || lower.contains("mac") {
            Some("macos")
        } else {
            None
        }
    })
}

/// Cargo writes a package name with `-` where the crate name has `_`; both name the same package here.
fn norm(package: &str) -> String {
    package.replace('-', "_")
}

/// The kind of test binary a suite names.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Target {
    /// The library's unit tests (`<package>`).
    Lib,
    /// A program's unit tests (`<package>::bin/<name>`).
    Bin,
    /// An integration test file (`<package>::<stem>`).
    Test(String),
}

/// A suite name split into its package (empty when the log did not say) and target.
fn split_suite(suite: &str) -> (String, Target) {
    match suite.split_once("::") {
        None => (norm(suite), Target::Lib),
        Some((p, rest)) if rest.starts_with("bin/") => (norm(p), Target::Bin),
        Some((p, rest)) => (norm(p), Target::Test(rest.to_string())),
    }
}

/// The last segment of a test path (`module::tests::name` is `name`).
fn last_segment(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name)
}

/// The results, indexed by OS, package, binary and test function name, so that the state of every item is a few
/// lookups however many cases there are.
#[derive(Debug, Default)]
pub struct Index {
    /// (os, package, target, function) -> (ran, every run passed).
    cases: BTreeMap<(String, String, Target, String), bool>,
}

impl Index {
    #[must_use]
    pub fn new(results: &Results) -> Index {
        let mut cases: BTreeMap<(String, String, Target, String), bool> = BTreeMap::new();
        for run in &results.runs {
            for case in &run.cases {
                let (package, target) = split_suite(&case.suite);
                let key = (
                    run.os.clone(),
                    package,
                    target,
                    last_segment(&case.name).to_string(),
                );
                let pass = case.outcome == Outcome::Pass;
                cases.entry(key).and_modify(|p| *p &= pass).or_insert(pass);
            }
        }
        Index { cases }
    }

    /// The outcome of one test on one OS: `Some(true)` when every matching case passed (at least one matched),
    /// `Some(false)` when one failed or was skipped, `None` when it did not run. `package` is the cargo package
    /// the test's file belongs to and `file` the path inside that package (`src/cli.rs`, `tests/golden.rs`).
    /// A suite whose package the log did not name (an integration test in cargo's older `deps/` layout) matches
    /// no test: it could be any package's, so it never counts.
    #[must_use]
    pub fn outcome(&self, package: &str, file: &str, function: &str, os: &str) -> Option<bool> {
        let targets = match file.strip_prefix("tests/") {
            Some(rest) => {
                let stem = rest
                    .split('/')
                    .next()
                    .unwrap_or(rest)
                    .trim_end_matches(".rs");
                vec![Target::Test(stem.to_string())]
            }
            // Sources under src/ are in the library's binary or one of the program binaries.
            None => vec![Target::Lib, Target::Bin],
        };
        let mut any = false;
        let mut all_pass = true;
        let package = norm(package);
        for t in &targets {
            let key = (
                os.to_string(),
                package.clone(),
                t.clone(),
                function.to_string(),
            );
            if let Some(p) = self.cases.get(&key) {
                any = true;
                all_pass &= *p;
            }
        }
        any.then_some(all_pass)
    }
}

/// The outcome of one test on one OS, for a repository of one package (see [`Index::outcome`]).
#[must_use]
pub fn outcome(results: &Results, package: &str, test: &TestRef, os: &str) -> Option<bool> {
    Index::new(results).outcome(package, &test.file, &test.function, os)
}

/// Drops terminal colour codes (`ESC [ ... m`) from a line of cargo's output.
fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for d in chars.by_ref() {
                    if d.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// The suite of a `Running <target> (<executable>)` line of `cargo test`, in the names cargo-nextest uses:
/// `<package>` for the library's unit tests, `<package>::bin/<name>` for a program's, `<package>::<stem>` for
/// `tests/<stem>.rs`. The package comes from the executable's path (`target/debug/build/<package>/<hash>/out/`,
/// cargo's newer layout) or, for unit tests in the older `target/debug/deps/<crate>-<hash>` layout, from the
/// crate name; an integration test in the older layout gets an empty package (and so never counts). `None` for a line that is not a
/// test binary (doc tests, benches, examples).
fn libtest_suite(rest: &str) -> Option<String> {
    let (target, exe) = rest.rsplit_once(" (")?;
    let exe = exe.trim_end_matches(')').replace('\\', "/");
    let target = target.trim().replace('\\', "/");
    let segments: Vec<&str> = exe.split('/').collect();
    let file = segments.last()?;
    let file = file.strip_suffix(".exe").unwrap_or(file);
    let stem = file.rsplit_once('-').map_or(file, |(s, _)| s);
    let package = segments
        .iter()
        .position(|s| *s == "build")
        .filter(|&i| segments.get(i + 3) == Some(&"out"))
        .and_then(|i| segments.get(i + 1))
        .map(|p| (*p).to_string());
    if let Some(unit) = target.strip_prefix("unittests ") {
        let package = package.unwrap_or_else(|| stem.to_string());
        if unit.ends_with("src/lib.rs") {
            return Some(package);
        }
        return Some(format!("{package}::bin/{stem}"));
    }
    let test = target.strip_prefix("tests/")?;
    let test_stem = test
        .split('/')
        .next()
        .unwrap_or(test)
        .trim_end_matches(".rs");
    Some(format!("{}::{test_stem}", package.unwrap_or_default()))
}

/// Reads the cases of the text `cargo test` prints (libtest's own output, colour codes and all): each
/// `Running ...` line starts a binary, each `test <name> ... ok|FAILED|ignored` line is a case of it. Doc tests,
/// benches and lines before the first binary are left out; a case without a result word (its output got in the
/// way) is left out too, so it never counts as passed.
///
/// # Errors
/// When the text has no `Running` line at all (not cargo test output).
pub fn parse_libtest(text: &str) -> Result<Vec<Case>, String> {
    let mut cases = Vec::new();
    let mut suite: Option<String> = None;
    let mut seen = false;
    for raw in text.lines() {
        let line = strip_ansi(raw);
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Running ") {
            seen = true;
            suite = libtest_suite(rest);
            continue;
        }
        if trimmed.starts_with("Doc-tests ") {
            seen = true;
            suite = None;
            continue;
        }
        let Some(s) = &suite else { continue };
        let Some(body) = line.strip_prefix("test ") else {
            continue;
        };
        let Some((name, result)) = body.rsplit_once(" ... ") else {
            continue;
        };
        let outcome = match result.trim() {
            "ok" => Outcome::Pass,
            "FAILED" => Outcome::Fail,
            r if r == "ignored" || r.starts_with("ignored, ") => Outcome::Skip,
            _ => continue,
        };
        cases.push(Case {
            suite: s.clone(),
            name: name.trim().to_string(),
            outcome,
        });
    }
    if !seen {
        return Err("not cargo test output: no `Running` line".into());
    }
    Ok(cases)
}

#[cfg(test)]
#[path = "results_tests.rs"]
mod tests;
