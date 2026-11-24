// SPDX-License-Identifier: GPL-3.0-only
//! Covers maps and workspaces: tests in files the repository does not edit, tagged from a TSV, and their results
//! from `cargo test` output of a workspace with several packages.

mod common;

use common::{Repo, covers, ids};

const UP_LIB: &str = r"pub fn file() {}

#[cfg(test)]
mod tests {
    #[test]
    fn upstream_reads_a_file() {}

    #[test]
    #[ignore = 'slow']
    fn upstream_slow() {}
}
";

/// The valid fixture plus an upstream package `up` (outside the scan paths) whose tests a map tags.
fn with_map(rows: &str) -> Repo {
    let r = Repo::valid();
    r.write(
        "crates/up/Cargo.toml",
        "[package]\nname = \"up-crate\"\nversion = \"0.1.0\"\n",
    );
    r.write("crates/up/src/lib.rs", &UP_LIB.replace('\'', "\""));
    let settings = r.read("catalogue/tally.toml");
    r.write(
        "catalogue/tally.toml",
        &format!("{settings}\n[[map]]\nfiles = \"catalogue/covers.d/*.tsv\"\n"),
    );
    r.write("catalogue/covers.d/up.tsv", rows);
    r.edit(
        "catalogue/ours/extra.toml",
        "title = \"Fast sort\"",
        "title = \"Fast sort\"\nstatus = \"implemented\"",
    );
    r
}

const ROW: &str = "test\tcovers\ncrates/up/src/lib.rs::upstream_reads_a_file\tours.extra.speed\n";

fn libtest(r: &Repo, os: &str, result: &str) -> String {
    let log = format!(
        "     Running unittests src/lib.rs (target/debug/build/up-crate/0a1b/out/up_crate-0a1b)\n\
         test tests::upstream_reads_a_file ... {result}\n\
         \u{1b}[1m     Running\u{1b}[0m unittests src/lib.rs (target/debug/build/demo/9f/out/demo-9f)\n\
         test tests::file_finds_a_file ... ok\n"
    );
    let path = format!("test-{os}.log");
    r.write(&path, &log);
    r.path(&path).to_string_lossy().into_owned()
}

fn ingest(r: &Repo, logs: &[String]) -> String {
    let out = r.path("results.json").to_string_lossy().into_owned();
    let mut args = vec!["ingest".to_string()];
    for l in logs {
        args.push("--libtest".into());
        args.push(l.clone());
    }
    args.push("--out".into());
    args.push(out.clone());
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let (code, text, err) = r.tally(&refs);
    assert_eq!(code, 0, "{text}{err}");
    out
}

fn report(r: &Repo, results: &str) -> String {
    let (code, out, err) = r.tally(&["report", "--depth", "3", "--results", results]);
    assert_eq!(code, 0, "{out}{err}");
    out
}

#[test]
fn a_map_row_tags_a_test_outside_the_scan_paths() {
    covers(&[ids::check::tags::MAP]);
    let r = with_map(ROW);
    r.passes(&[]);
    let (_, out, _) = r.tally(&["report", "--depth", "3"]);
    assert!(out.contains("tagged      ours.extra.speed"), "{out}");
}

#[test]
fn a_mapped_test_that_passed_on_three_platforms_in_its_own_package_is_covered() {
    covers(&[
        ids::results::workspace::PACKAGE,
        ids::results::libtest::CASES,
        ids::cmd::cli::INGEST,
    ]);
    let r = with_map(ROW);
    let logs: Vec<String> = ["linux", "windows", "macos"]
        .iter()
        .map(|os| libtest(&r, os, "ok"))
        .collect();
    let res = ingest(&r, &logs);
    let out = report(&r, &res);
    assert!(out.contains("covered     ours.extra.speed"), "{out}");
    // The root package's own test is found in its own suite.
    assert!(out.contains("covered     cond.func.file"), "{out}");
    let logs: Vec<String> = ["linux", "windows"]
        .iter()
        .map(|os| libtest(&r, os, "ok"))
        .chain([libtest(&r, "macos", "FAILED")])
        .collect();
    let res = ingest(&r, &logs);
    let out = report(&r, &res);
    assert!(out.contains("passing     ours.extra.speed"), "{out}");
}

#[test]
fn a_test_of_one_package_does_not_answer_for_another() {
    covers(&[ids::results::workspace::PACKAGE]);
    let r = with_map(ROW);
    // The same test name, run in the root package's binary only.
    let logs: Vec<String> = ["linux", "windows", "macos"]
        .iter()
        .map(|os| {
            let path = format!("other-{os}.log");
            r.write(
                &path,
                "     Running unittests src/lib.rs (target/debug/build/demo/9f/out/demo-9f)\ntest tests::upstream_reads_a_file ... ok\n",
            );
            r.path(&path).to_string_lossy().into_owned()
        })
        .collect();
    let res = ingest(&r, &logs);
    let out = report(&r, &res);
    assert!(out.contains("tagged      ours.extra.speed"), "{out}");
}

#[test]
fn a_map_row_that_names_no_test_is_refused() {
    covers(&[ids::check::tags::MAP]);
    let r = with_map("test\tcovers\ncrates/up/src/lib.rs::no_such_test\tours.extra.speed\n");
    r.fails_with("tag-data");
    let r =
        with_map("test\tcovers\ncrates/up/src/gone.rs::upstream_reads_a_file\tours.extra.speed\n");
    r.fails_with("tag-data");
    let r = with_map("test\tcovers\n../outside.rs::x\tours.extra.speed\n");
    r.fails_with("tag-data");
}

#[test]
fn a_map_with_a_bad_header_or_row_is_refused() {
    covers(&[ids::check::tags::MAP]);
    for text in [
        "name\tcovers\ncrates/up/src/lib.rs::upstream_reads_a_file\tours.extra.speed\n",
        "test\tcovers\tnotes\ncrates/up/src/lib.rs::upstream_reads_a_file\tours.extra.speed\n",
        "test\tcovers\ncrates/up/src/lib.rs\tours.extra.speed\n",
        "test\tcovers\ncrates/up/src/lib.rs::upstream_reads_a_file\t\n",
        "test\tcovers\ncrates/up/src/lib.rs::upstream_reads_a_file\tours.extra.speed\tsneaked reason\n",
        "test\tcovers\n./crates/up/src/lib.rs::upstream_reads_a_file\tours.extra.speed\n",
        "test\tcovers\ncrates//up/src/lib.rs::upstream_reads_a_file\tours.extra.speed\n",
        "test\tcovers\ncrates\\up\\src\\lib.rs::upstream_reads_a_file\tours.extra.speed\n",
        "",
    ] {
        let r = with_map(text);
        r.fails_with("tag-data");
    }
    let r = Repo::valid();
    let settings = r.read("catalogue/tally.toml");
    r.write(
        "catalogue/tally.toml",
        &format!("{settings}\n[[map]]\nfiles = \"catalogue/covers.d/*.tsv\"\n"),
    );
    r.fails_with("tag-data");
}

#[test]
fn a_map_row_must_name_live_ids() {
    covers(&[ids::check::tags::MAP, ids::check::tags::UNKNOWN]);
    let r =
        with_map("test\tcovers\ncrates/up/src/lib.rs::upstream_reads_a_file\tours.extra.nope\n");
    r.fails_with("tag-unknown");
}

#[test]
fn a_mapped_ignored_test_is_warned_about_and_not_counted() {
    covers(&[ids::check::tags::MAP, ids::check::tags::IGNORED]);
    let r = with_map("test\tcovers\ncrates/up/src/lib.rs::upstream_slow\tours.extra.speed\n");
    let out = r.passes(&[]);
    assert!(out.contains("warning[tag-ignored]"), "{out}");
    let (_, report, _) = r.tally(&["report", "--depth", "3"]);
    assert!(report.contains("implemented ours.extra.speed"), "{report}");
}

#[test]
fn a_map_row_with_more_than_ten_ids_needs_a_reason() {
    covers(&[ids::check::tags::MAP_BULK]);
    let ids = ["ours.extra.speed"; 11].join(",");
    let r = with_map(&format!(
        "test\tcovers\ncrates/up/src/lib.rs::upstream_reads_a_file\t{ids}\n"
    ));
    r.fails_with("tag-bulk");
    let r = with_map(&format!(
        "test\tcovers\ttable\ncrates/up/src/lib.rs::upstream_reads_a_file\t{ids}\tone call per command\n"
    ));
    r.passes(&[]);
    // Two rows of six ids for one test are one test of twelve.
    let six = ["ours.extra.speed"; 6].join(" ");
    let r = with_map(&format!(
        "test\tcovers\ncrates/up/src/lib.rs::upstream_reads_a_file\t{six}\ncrates/up/src/lib.rs::upstream_reads_a_file\t{six}\n"
    ));
    r.fails_with("tag-bulk");
}
