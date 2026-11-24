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
    PLATFORMS
        .iter()
        .copied()
        .find(|p| lower.contains(p))
        .or_else(|| {
            if lower.contains("ubuntu") {
                Some("linux")
            } else if lower.contains("darwin") || lower.contains("mac") {
                Some("macos")
            } else {
                None
            }
        })
}

/// The test binaries a source file's tests are in.
fn suites_for(package: &str, file: &str) -> (Vec<String>, bool) {
    if let Some(rest) = file.strip_prefix("tests/") {
        let stem = rest
            .split('/')
            .next()
            .unwrap_or(rest)
            .trim_end_matches(".rs");
        return (vec![format!("{package}::{stem}")], false);
    }
    // Sources under src/ are in the library's binary or one of the program binaries.
    (vec![package.to_string()], true)
}

/// The outcome of one test on one OS: `Some(true)` when every matching case passed (at least one matched),
/// `Some(false)` when one failed or was skipped, `None` when it did not run.
#[must_use]
pub fn outcome(results: &Results, package: &str, test: &TestRef, os: &str) -> Option<bool> {
    let (suites, bins) = suites_for(package, &test.file);
    let bin_prefix = format!("{package}::bin/");
    let mut any = false;
    let mut all_pass = true;
    for run in results.runs.iter().filter(|r| r.os == os) {
        for case in &run.cases {
            let suite_ok =
                suites.contains(&case.suite) || (bins && case.suite.starts_with(&bin_prefix));
            let name_ok =
                case.name == test.function || case.name.ends_with(&format!("::{}", test.function));
            if suite_ok && name_ok {
                any = true;
                all_pass &= case.outcome == Outcome::Pass;
            }
        }
    }
    any.then_some(all_pass)
}

#[cfg(test)]
#[path = "results_tests.rs"]
mod tests;
