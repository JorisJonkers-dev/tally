// SPDX-License-Identifier: GPL-3.0-only
//! The commands other than `check`, run on the fixture repository.

mod common;

use common::{Repo, UPSTREAM, covers, ids, sha};

const JUNIT_PASS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="2" failures="0" errors="0">
  <testsuite name="demo" tests="1">
    <testcase name="tests::file_finds_a_file" classname="demo" time="0.001"/>
  </testsuite>
  <testsuite name="demo::cases" tests="1">
    <testcase name="every_case_passes" classname="demo::cases" time="0.001">
      <system-out><![CDATA[<testcase name="fake"/>]]></system-out>
    </testcase>
  </testsuite>
</testsuites>
"#;

fn ingest_all(r: &Repo, linux: &str, windows: &str, macos: &str) -> String {
    for (os, xml) in [("linux", linux), ("windows", windows), ("macos", macos)] {
        r.write(&format!("junit-{os}.xml"), xml);
    }
    let out = r.path("results.json").to_string_lossy().into_owned();
    let files: Vec<String> = ["linux", "windows", "macos"]
        .iter()
        .map(|os| {
            r.path(&format!("junit-{os}.xml"))
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let (code, text, err) = r.tally(&[
        "ingest", "--junit", &files[0], "--junit", &files[1], "--junit", &files[2], "--out", &out,
    ]);
    assert_eq!(code, 0, "{text}{err}");
    out
}

fn report(r: &Repo, args: &[&str]) -> String {
    let mut all = vec!["report"];
    all.extend_from_slice(args);
    let (code, out, err) = r.tally(&all);
    assert_eq!(code, 0, "{out}{err}");
    out
}

#[test]
fn without_results_a_tagged_item_is_tagged() {
    covers(&[ids::cmd::cli::REPORT, ids::report::format::TERM]);
    let r = Repo::valid();
    let out = report(&r, &["--depth", "3"]);
    assert!(out.contains("tagged      cond.func.file"), "{out}");
    assert!(out.contains("missing     ours.extra.speed"), "{out}");
    assert!(out.contains("no test results given"), "{out}");
}

#[test]
fn a_test_that_passed_on_all_three_platforms_makes_its_item_covered() {
    covers(&[
        ids::results::junit::COVERED,
        ids::results::junit::PASS,
        ids::cmd::cli::INGEST,
    ]);
    let r = Repo::valid();
    let res = ingest_all(&r, JUNIT_PASS, JUNIT_PASS, JUNIT_PASS);
    let out = report(&r, &["--depth", "3", "--results", &res]);
    assert!(out.contains("covered     cond.func.file"), "{out}");
    assert!(out.contains("covered     cond.func.active"), "{out}");
}

#[test]
fn a_test_that_passed_on_linux_only_is_passing_not_covered() {
    covers(&[ids::results::junit::COVERED]);
    let r = Repo::valid();
    let res = ingest_all(&r, JUNIT_PASS, "<testsuites/>", "<testsuites/>");
    let out = report(&r, &["--depth", "3", "--results", &res]);
    assert!(out.contains("passing     cond.func.file"), "{out}");
}

#[test]
fn a_failed_test_does_not_pass() {
    covers(&[ids::results::junit::FAILURE]);
    let r = Repo::valid();
    let failed = JUNIT_PASS.replace(
        r#"<testcase name="tests::file_finds_a_file" classname="demo" time="0.001"/>"#,
        r#"<testcase name="tests::file_finds_a_file" classname="demo"><failure message="x">boom</failure></testcase>"#,
    );
    let res = ingest_all(&r, &failed, JUNIT_PASS, JUNIT_PASS);
    let out = report(&r, &["--depth", "3", "--results", &res]);
    assert!(out.contains("passing     cond.func.file"), "{out}");
}

#[test]
fn a_skipped_test_never_counts() {
    covers(&[ids::results::junit::SKIPPED]);
    let r = Repo::valid();
    let skipped = JUNIT_PASS.replace(
        r#"<testcase name="tests::file_finds_a_file" classname="demo" time="0.001"/>"#,
        r#"<testcase name="tests::file_finds_a_file" classname="demo"><skipped/></testcase>"#,
    );
    let res = ingest_all(&r, &skipped, &skipped, &skipped);
    let out = report(&r, &["--depth", "3", "--results", &res]);
    assert!(out.contains("tagged      cond.func.file"), "{out}");
}

#[test]
fn a_test_that_needed_a_retry_never_counts() {
    covers(&[ids::results::junit::FLAKY]);
    let r = Repo::valid();
    let flaky = JUNIT_PASS.replace(
        r#"<testcase name="tests::file_finds_a_file" classname="demo" time="0.001"/>"#,
        r#"<testcase name="tests::file_finds_a_file" classname="demo"><flakyFailure message="x"/></testcase>"#,
    );
    let res = ingest_all(&r, &flaky, &flaky, &flaky);
    let out = report(&r, &["--depth", "3", "--results", &res]);
    assert!(out.contains("tagged      cond.func.file"), "{out}");
}

#[test]
fn ingest_without_an_os_in_the_name_needs_os() {
    covers(&[ids::results::junit::OS, ids::cmd::cli::USAGE]);
    let r = Repo::valid();
    r.write("report.xml", JUNIT_PASS);
    let file = r.path("report.xml").to_string_lossy().into_owned();
    let out = r.path("res.json").to_string_lossy().into_owned();
    let (code, _, err) = r.tally(&["ingest", "--junit", &file, "--out", &out]);
    assert_eq!(code, 2, "{err}");
    let (code, _, err) = r.tally(&["ingest", "--junit", &file, "--os", "windows", "--out", &out]);
    assert_eq!(code, 0, "{err}");
    assert!(r.read("res.json").contains("\"os\": \"windows\""));
}

#[test]
fn ingest_append_keeps_the_earlier_runs() {
    covers(&[ids::cmd::cli::INGEST]);
    let r = Repo::valid();
    r.write("a.xml", JUNIT_PASS);
    let file = r.path("a.xml").to_string_lossy().into_owned();
    let out = r.path("res.json").to_string_lossy().into_owned();
    r.tally(&["ingest", "--junit", &file, "--os", "linux", "--out", &out]);
    r.tally(&[
        "ingest", "--junit", &file, "--os", "macos", "--out", &out, "--append",
    ]);
    let text = r.read("res.json");
    assert!(
        text.contains("\"linux\"") && text.contains("\"macos\""),
        "{text}"
    );
}

#[test]
fn ingest_refuses_a_file_that_is_not_junit() {
    covers(&[ids::cmd::cli::INGEST]);
    let r = Repo::valid();
    r.write("linux.xml", "<html></html>");
    let file = r.path("linux.xml").to_string_lossy().into_owned();
    let (code, _, err) = r.tally(&["ingest", "--junit", &file]);
    assert_eq!(code, 2);
    assert!(err.contains("not a JUnit report"), "{err}");
}

#[test]
fn a_results_file_of_another_schema_is_refused() {
    covers(&[ids::cmd::cli::USAGE]);
    let r = Repo::valid();
    r.write("res.json", "{\"schema\":\"other\",\"runs\":[]}");
    let file = r.path("res.json").to_string_lossy().into_owned();
    let (code, _, err) = r.tally(&["report", "--results", &file]);
    assert_eq!(code, 2, "{err}");
}

#[test]
fn depth_one_shows_areas_only() {
    covers(&[ids::report::format::DEPTH]);
    let r = Repo::valid();
    let out = report(&r, &["--depth", "1"]);
    assert!(out.contains("cond "), "{out}");
    assert!(!out.contains("cond.func"), "{out}");
}

#[test]
fn depth_two_shows_groups_without_items() {
    covers(&[ids::report::format::DEPTH]);
    let r = Repo::valid();
    let out = report(&r, &[]);
    assert!(out.contains("  cond.func"), "{out}");
    assert!(!out.contains("cond.func.file"), "{out}");
}

#[test]
fn the_missing_filter_keeps_only_missing_items() {
    covers(&[ids::report::format::MISSING]);
    let r = Repo::valid();
    let out = report(&r, &["--depth", "3", "--missing"]);
    assert!(out.contains("ours.extra.speed"), "{out}");
    assert!(!out.contains("cond.func"), "{out}");
}

#[test]
fn the_origin_filter_keeps_only_that_origin() {
    covers(&[ids::report::format::ORIGIN]);
    let r = Repo::valid();
    let out = report(&r, &["--depth", "3", "--origin", "ours"]);
    assert!(
        out.contains("ours.extra.speed") && !out.contains("cond.func"),
        "{out}"
    );
    let out = report(&r, &["--depth", "3", "--origin", "original"]);
    assert!(
        !out.contains("ours.extra") && out.contains("cond.func.file"),
        "{out}"
    );
}

#[test]
fn the_json_report_has_the_schema_and_totals() {
    covers(&[ids::report::format::JSON]);
    let r = Repo::valid();
    let out = report(&r, &["--format", "json", "--depth", "3"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["schema"], "tally-report.v1");
    assert_eq!(v["totals"]["total"], 3);
    assert_eq!(v["totals"]["implemented"], 2);
    assert_eq!(v["totals"]["tagged"], 2);
    assert_eq!(v["totals"]["not_applicable"], 1);
    assert_eq!(v["areas"][0]["title"], "Conditions");
    assert_eq!(
        v["areas"][0]["groups"][0]["items"][0]["tests"][0],
        "src/lib.rs::file_finds_a_file"
    );
}

#[test]
fn a_json_report_is_a_valid_ratchet_for_check() {
    covers(&[ids::report::format::JSON, ids::check::history::RATCHET]);
    let r = Repo::valid();
    let path = r.path("report.json").to_string_lossy().into_owned();
    report(&r, &["--format", "json", "--out", &path]);
    r.passes(&["--ratchet", &path]);
    r.edit("src/lib.rs", "covers(&[ids::cond::func::FILE]);", "");
    r.fails_with_args("ratchet", &["--ratchet", &path]);
}

#[test]
fn the_markdown_report_has_a_row_per_group_and_item_tables() {
    covers(&[ids::report::format::MD]);
    let r = Repo::valid();
    let out = report(&r, &["--format", "md", "--depth", "3"]);
    assert!(out.contains("| **Total** | 2/3 |"), "{out}");
    assert!(
        out.contains("| Condition functions (`cond.func`) |"),
        "{out}"
    );
    assert!(
        out.contains(
            "| `file(path)` | `cond.func.file` | tagged | `src/lib.rs::file_finds_a_file` |"
        ),
        "{out}"
    );
}

#[test]
fn the_svg_report_is_a_dark_panel_with_a_bar_per_area_and_its_counts() {
    covers(&[ids::report::format::SVG]);
    let r = Repo::valid();
    let out = report(&r, &["--format", "svg"]);
    assert!(out.starts_with("<svg"), "{out}");
    assert!(out.contains("fill=\"#14110b\""), "{out}");
    assert!(out.contains("DEMO · FEATURE CATALOGUE"), "{out}");
    // One track per row: all areas, Conditions, ours.
    assert_eq!(out.matches("fill=\"#2b2418\"/>").count(), 3, "{out}");
    assert!(out.contains(">3 items · 0.0% covered</text>"), "{out}");
    // The legend gives each state's count; each takes in the states before it.
    for text in [
        ">0 covered on all 3 systems</text>",
        ">0 passing on any system</text>",
        ">2 tagged</text>",
        ">2 implemented</text>",
        ">1 missing</text>",
    ] {
        assert!(out.contains(text), "{text} in {out}");
    }
    assert!(out.contains(">1 item · 0.0% covered</text>"), "{out}");
    assert!(out.contains(">1 n/a or deferred</text>"), "{out}");
}

#[test]
fn the_svg_report_lists_the_missing_items_of_each_area() {
    covers(&[ids::report::format::SVG]);
    let out = report(&Repo::valid(), &["--format", "svg"]);
    assert!(out.contains(">gaps: ours.extra.speed</text>"), "{out}");
}

#[test]
fn the_svg_report_draws_covered_items_solid() {
    covers(&[ids::report::format::SVG, ids::results::junit::COVERED]);
    let r = Repo::valid();
    let res = ingest_all(&r, JUNIT_PASS, JUNIT_PASS, JUNIT_PASS);
    let out = report(&r, &["--format", "svg", "--results", &res]);
    assert!(
        out.contains("width=\"760\" height=\"12\" fill=\"#f2c14e\""),
        "{out}"
    );
    assert!(
        out.contains("covered: a tagged test passed on Linux, Windows and macOS"),
        "{out}"
    );
}

#[test]
fn the_svg_report_stacks_passing_items_after_covered_ones() {
    covers(&[ids::report::format::SVG]);
    let r = Repo::valid();
    // Linux and Windows pass, macOS ran nothing: both tagged items are passing, none is covered.
    let none = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="0" failures="0" errors="0">
</testsuites>
"#;
    let res = ingest_all(&r, JUNIT_PASS, JUNIT_PASS, none);
    let out = report(&r, &["--format", "svg", "--results", &res]);
    assert!(out.contains(">2 passing on any system</text>"), "{out}");
    assert!(out.contains(">0 covered on all 3 systems</text>"), "{out}");
    // The headline counts 3 items: the 2 passing items fill two thirds of the bar, from its start.
    assert!(
        out.contains("<rect x=\"20\" y=\"66\" width=\"507\" height=\"12\" fill=\"#d9862b\""),
        "{out}"
    );
    assert!(!out.contains("height=\"12\" fill=\"#f2c14e\""), "{out}");
}

#[test]
fn the_svg_report_cuts_a_long_area_title_before_the_counts() {
    covers(&[ids::report::format::SVG]);
    let r = Repo::valid();
    let long = "Conditions ".repeat(12);
    r.edit(
        "catalogue/tally.toml",
        "cond = \"Conditions\"",
        &format!("cond = \"{long}\""),
    );
    let out = report(&r, &["--format", "svg"]);
    let line = out
        .lines()
        .find(|l| l.contains(">Conditions Conditions"))
        .expect("the area title is drawn");
    let title = line
        .split_once("\">")
        .and_then(|(_, t)| t.strip_suffix("</text>"))
        .expect("one text element");
    assert!(title.ends_with('…'), "{line}");
    // 760 px less the counts ("2 items · 0.0% covered", 22 characters at 7.8 px = 172 px) and a 20 px gap leaves
    // 568 px: 72 characters.
    assert_eq!(title.chars().count(), 72, "{line}");
}

#[test]
fn the_svg_report_is_the_same_bytes_every_time() {
    covers(&[ids::report::format::SVG]);
    let r = Repo::valid();
    assert_eq!(
        report(&r, &["--format", "svg"]),
        report(&r, &["--format", "svg", "--depth", "1"])
    );
}

#[test]
fn ids_out_writes_one_constant_per_live_item() {
    covers(&[ids::cmd::cli::IDS_OUT, ids::typed::ids::MODULE]);
    let r = Repo::valid();
    let path = r.path("ids.rs").to_string_lossy().into_owned();
    let (code, _, err) = r.tally(&["ids", "--out", &path]);
    assert_eq!(code, 0, "{err}");
    let src = r.read("ids.rs");
    assert!(
        src.contains(
            "pub const FILE: super::super::super::Id = super::super::super::Id(\"cond.func.file\");"
        ),
        "{src}"
    );
    assert!(src.contains("pub const OTHER_GAME"), "{src}");
}

#[test]
fn ids_generator_writes_the_copy_check_accepts() {
    covers(&[ids::cmd::cli::IDS_GENERATOR]);
    let r = Repo::valid();
    r.remove("catalogue/tally_ids_gen.rs");
    let path = r
        .path("catalogue/tally_ids_gen.rs")
        .to_string_lossy()
        .into_owned();
    let (code, _, err) = r.tally(&["ids", "--generator", &path]);
    assert_eq!(code, 0, "{err}");
    r.passes(&[]);
}

#[test]
fn ids_needs_exactly_one_target() {
    covers(&[ids::cmd::cli::USAGE]);
    let (code, _, _) = Repo::valid().tally(&["ids"]);
    assert_eq!(code, 2);
}

const DOC: &str = "# Compat\n\nBefore.\n\n<!-- tally:group cond.func -->\nold table\n<!-- tally:end -->\n\nAfter.\n";

#[test]
fn doc_replaces_the_marked_block_with_the_group_table() {
    covers(&[ids::cmd::cli::DOC]);
    let r = Repo::valid();
    r.write("COMPAT.md", DOC);
    let file = r.path("COMPAT.md").to_string_lossy().into_owned();
    let (code, out, err) = r.tally(&["doc", &file, "--check"]);
    assert_eq!(code, 1, "{out}{err}");
    let (code, _, err) = r.tally(&["doc", &file]);
    assert_eq!(code, 0, "{err}");
    let text = r.read("COMPAT.md");
    assert!(text.starts_with("# Compat\n\nBefore.\n\n<!-- tally:group cond.func -->\n| Feature | Status | Notes | Id |"), "{text}");
    assert!(
        text.contains("| `active(path)` | supported, except regex |  | `active` |"),
        "{text}"
    );
    assert!(text.ends_with("<!-- tally:end -->\n\nAfter.\n"), "{text}");
    let (code, out, _) = r.tally(&["doc", &file, "--check"]);
    assert_eq!(code, 0, "{out}");
}

#[test]
fn doc_refuses_an_unknown_group() {
    covers(&[ids::cmd::cli::DOC]);
    let r = Repo::valid();
    r.write("COMPAT.md", &DOC.replace("cond.func", "cond.nope"));
    let file = r.path("COMPAT.md").to_string_lossy().into_owned();
    let (code, _, err) = r.tally(&["doc", &file]);
    assert_eq!(code, 2);
    assert!(err.contains("cond.nope"), "{err}");
}

#[test]
fn refs_check_passes_on_good_pins_and_fails_on_a_changed_list() {
    covers(&[ids::cmd::cli::REFS_CHECK]);
    let r = Repo::valid();
    let (code, out, _) = r.tally(&["refs", "check"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("1 sources, 2 reference rows"), "{out}");
    r.write("catalogue/ref/up.tsv", "key\ttitle\nfile\tfile(path)\n");
    let (code, out, _) = r.tally(&["refs", "check"]);
    assert_eq!(code, 1, "{out}");
}

fn file_url(p: &std::path::Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{s}")
    }
}

fn upstream(r: &Repo) -> String {
    let base = r.path("upstream");
    let commit = "0123456789abcdef0123456789abcdef01234567";
    r.write(&format!("upstream/{commit}/docs/conditions.md"), UPSTREAM);
    file_url(&base)
}

#[test]
fn refs_fetch_downloads_the_pinned_files_and_checks_their_hashes() {
    covers(&[ids::cmd::cli::REFS_FETCH]);
    let r = Repo::valid();
    let url = upstream(&r);
    r.edit(
        "catalogue/sources.toml",
        "[source.upstream]",
        &format!("raw = \"{url}\"\n\n[source.upstream]"),
    );
    let cache = r.path("cache").to_string_lossy().into_owned();
    let (code, out, err) = r.tally(&["refs", "fetch", "--cache", &cache]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("docs/conditions.md matches its pin"), "{out}");
    r.write(
        "upstream/0123456789abcdef0123456789abcdef01234567/docs/conditions.md",
        "changed upstream\n",
    );
    let (code, out, _) = r.tally(&["refs", "fetch", "--cache", &cache]);
    assert_eq!(code, 1, "{out}");
}

#[test]
fn refs_fetch_rebuilds_a_tsv_list_and_writes_it_with_update() {
    covers(&[ids::cmd::cli::REFS_FETCH, ids::refs::importers::TSV]);
    let r = Repo::valid();
    let url = upstream(&r);
    let table =
        "addr\tname\tstate\n0x1\tfile(path)\tdone\n0x2\tactive(path)\tdone\n0x3\tmany\topen\n";
    r.write(
        "upstream/0123456789abcdef0123456789abcdef01234567/map.tsv",
        table,
    );
    let text = r
        .read("catalogue/sources.toml")
        .replace("extractor = \"manual\"", "extractor = \"tsv\"");
    let text = text.replace(
        "[source.upstream]",
        &format!(
            "raw = \"{url}\"\n\n[source.upstream]\n\"map.tsv\" = \"{}\"",
            sha(table)
        ),
    );
    r.write("catalogue/sources.toml", &format!("{text}\n[source.tsv]\nfrom = \"map.tsv\"\nkey_column = \"addr\"\ntitle_column = \"name\"\nwhere_column = \"state\"\nwhere_value = \"done\"\n"));
    let cache = r.path("cache").to_string_lossy().into_owned();
    let (code, out, _) = r.tally(&["refs", "fetch", "--cache", &cache]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--update"), "{out}");
    let (code, out, _) = r.tally(&["refs", "fetch", "--cache", &cache, "--update"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        r.read("catalogue/ref/up.tsv"),
        "key\ttitle\n0x1\tfile(path)\n0x2\tactive(path)\n"
    );
    assert!(
        out.contains(&sha("key\ttitle\n0x1\tfile(path)\n0x2\tactive(path)\n")),
        "{out}"
    );
}

#[test]
fn refs_fetch_rebuilds_a_ledger_list_and_writes_it_with_update() {
    covers(&[ids::cmd::cli::REFS_FETCH, ids::refs::importers::LEDGER]);
    let r = Repo::valid();
    let url = upstream(&r);
    let map = "# FalloutNV.exe\naddress\tsize\tname\n00401020\t10\t\n00401000\t21\tFoo\n";
    r.write(
        "upstream/0123456789abcdef0123456789abcdef01234567/engine_map.tsv",
        map,
    );
    let text = r
        .read("catalogue/sources.toml")
        .replace("extractor = \"manual\"", "extractor = \"ledger\"");
    let text = text.replace(
        "[source.upstream]",
        &format!(
            "raw = \"{url}\"\n\n[source.upstream]\n\"engine_map.tsv\" = \"{}\"",
            sha(map)
        ),
    );
    r.write(
        "catalogue/sources.toml",
        &format!("{text}\n[source.ledger]\nfrom = \"engine_map.tsv\"\n"),
    );
    let cache = r.path("cache").to_string_lossy().into_owned();
    let (code, out, _) = r.tally(&["refs", "fetch", "--cache", &cache, "--update"]);
    assert_eq!(code, 0, "{out}");
    let list = "key\ttitle\nf00401000\tFoo\nf00401020\t0x00401020\n";
    assert_eq!(r.read("catalogue/ref/up.tsv"), list);
    assert!(out.contains(&sha(list)), "{out}");
}

#[test]
fn refs_fetch_of_an_unknown_source_fails() {
    covers(&[ids::cmd::cli::REFS_FETCH]);
    let r = Repo::valid();
    let (code, out, _) = r.tally(&["refs", "fetch", "--source", "nope", "--cache", "x"]);
    assert_eq!(code, 1, "{out}");
}

#[test]
fn wrong_arguments_exit_two_with_the_usage() {
    covers(&[ids::cmd::cli::USAGE]);
    let r = Repo::valid();
    for args in [
        vec!["nope"],
        vec![],
        vec!["report", "--depth", "4"],
        vec!["report", "--format", "pdf"],
        vec!["report", "--origin", "theirs"],
        vec!["report", "--extra"],
        vec!["check", "--allow-drop", "many"],
        vec!["check", "--base"],
        vec!["refs", "list"],
        vec!["ingest"],
        vec!["ingest", "--junit", "x.xml", "--os", "beos"],
        vec!["doc"],
    ] {
        let (code, _, err) = r.tally(&args);
        assert_eq!(code, 2, "{args:?}: {err}");
        assert!(err.contains("usage:"), "{args:?}: {err}");
    }
}

#[test]
fn a_missing_catalogue_is_a_usage_error() {
    covers(&[ids::cmd::cli::USAGE]);
    let r = Repo::valid();
    std::fs::remove_dir_all(r.path("catalogue")).unwrap();
    let (code, _, err) = r.tally(&["check"]);
    assert_eq!(code, 2);
    assert!(err.contains("no catalogue directory"), "{err}");
}

#[test]
fn version_and_help_exit_zero() {
    covers(&[ids::cmd::cli::USAGE]);
    let mut out = Vec::new();
    let mut err = Vec::new();
    assert_eq!(
        tally::cli::run(&["--version".into()], &mut out, &mut err),
        0
    );
    assert!(String::from_utf8_lossy(&out).starts_with("tally "));
    assert_eq!(tally::cli::run(&["help".into()], &mut out, &mut err), 0);
}

#[test]
fn an_area_left_out_of_the_total_keeps_its_own_row_but_not_the_headline() {
    covers(&[ids::report::format::HEADLINE]);
    let r = Repo::valid();
    r.edit(
        "catalogue/tally.toml",
        "[areas]",
        "[report]\nleave_out_of_total = [\"cond\"]\n\n[areas]",
    );
    assert_eq!(r.check(&[]).0, 0, "{}", r.check(&[]).1);
    let svg = report(&r, &["--format", "svg"]);
    // The picture keeps every area's row and says in the first row which area it does not count.
    assert!(
        svg.contains(">All areas · not counting Conditions</text>"),
        "{svg}"
    );
    assert!(!svg.contains(">All areas</text>"), "{svg}");
    // Headline (ours: 1 item, the n/a item is not counted), Conditions (2 items), ours.
    assert_eq!(svg.matches("fill=\"#2b2418\"/>").count(), 3, "{svg}");
    assert_eq!(
        svg.matches(">1 item · 0.0% covered</text>").count(),
        2,
        "{svg}"
    );
    assert!(svg.contains(">2 items · 0.0% covered</text>"), "{svg}");
    let json: serde_json::Value =
        serde_json::from_str(&report(&r, &["--format", "json"])).expect("json");
    // The ratchet still sees every item.
    assert_eq!(json["totals"]["total"], 3, "{json}");
    assert_eq!(json["headline"]["counts"]["total"], 1, "{json}");
    assert_eq!(
        json["headline"]["left_out"],
        serde_json::json!(["cond"]),
        "{json}"
    );
    // A report with the area left out is still a ratchet over every item: dropping a tag in that area fails.
    let path = r.path("report.json").to_string_lossy().into_owned();
    report(&r, &["--format", "json", "--out", &path]);
    r.edit("src/lib.rs", "covers(&[ids::cond::func::FILE]);", "");
    r.fails_with_args("ratchet", &["--ratchet", &path]);
    let md = report(&r, &["--format", "md"]);
    assert!(
        md.contains("| **Total, except Conditions** | 0/1 |"),
        "{md}"
    );
    let term = report(&r, &[]);
    assert!(term.contains("total except cond"), "{term}");
}

#[test]
fn a_hidden_total_leaves_only_the_area_rows() {
    covers(&[ids::report::format::HEADLINE]);
    let r = Repo::valid();
    r.edit(
        "catalogue/tally.toml",
        "[areas]",
        "[report]\ntotal = false\n\n[areas]",
    );
    let svg = report(&r, &["--format", "svg"]);
    assert!(!svg.contains(">All areas"), "{svg}");
    assert_eq!(svg.matches("fill=\"#2b2418\"/>").count(), 2, "{svg}");
    let json: serde_json::Value =
        serde_json::from_str(&report(&r, &["--format", "json"])).expect("json");
    assert!(json.get("headline").is_none(), "{json}");
    assert_eq!(json["totals"]["total"], 3, "{json}");
    assert!(!report(&r, &["--format", "md"]).contains("**Total"));
    assert!(!report(&r, &[]).contains("\ntotal "));
}

#[test]
fn leaving_out_an_area_the_catalogue_does_not_have_is_an_error() {
    covers(&[ids::report::format::HEADLINE]);
    let r = Repo::valid();
    r.edit(
        "catalogue/tally.toml",
        "[areas]",
        "[report]\nleave_out_of_total = [\"engine\"]\n\n[areas]",
    );
    let out = r.fails_with("settings");
    assert!(out.contains("`engine`"), "{out}");
}

/// The valid repository with an evidence file whose rows say which missing items work another way and what share
/// of each item is done.
fn with_extents() -> Repo {
    let r = Repo::valid();
    r.write(
        "evidence.tsv",
        "id\tstate\tshare\n\
         cond.func.file\timplemented\t3/0/3\n\
         cond.func.active\timplemented\t1/1/2\n\
         ours.extra.speed\tfallback\t1/2/4\n",
    );
    let settings = r.read("catalogue/tally.toml");
    r.write(
        "catalogue/tally.toml",
        &format!(
            "{settings}\n[report]\nworks_label = \"works via the old code\"\nshare_label = \"native\"\n\n\
             [[evidence]]\nkind = \"tsv\"\npath = \"evidence.tsv\"\nstatus_column = \"state\"\n\
             implemented = [\"implemented\"]\nworks = [\"fallback\"]\nshare_column = \"share\"\n\
             groups = [\"cond.func\", \"ours.extra\"]\n"
        ),
    );
    r
}

#[test]
fn a_catalogue_with_works_rows_and_shares_passes_the_check() {
    covers(&[ids::check::evidence::WORKS_SHARE]);
    with_extents().passes(&[]);
}

#[test]
fn a_malformed_share_or_works_on_another_importer_fails_the_check() {
    covers(&[ids::check::evidence::WORKS_SHARE]);
    let r = with_extents();
    r.edit("evidence.tsv", "1/2/4", "3/2/4");
    r.fails_with("evidence-file");
    let r = with_extents();
    r.edit(
        "catalogue/tally.toml",
        "kind = \"tsv\"",
        "kind = \"contracts\"",
    );
    r.fails_with("evidence-kind");
}

#[test]
fn the_json_report_counts_items_that_work_another_way_and_averages_shares() {
    covers(&[ids::report::format::WORKS, ids::report::format::SHARE]);
    let out = report(&with_extents(), &["--format", "json", "--depth", "3"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let ours = &v["areas"][1];
    assert_eq!(ours["id"], "ours");
    assert_eq!(ours["counts"]["works"], 1);
    assert_eq!(ours["counts"]["missing"], 1);
    // speed: 1 of 4 done, 2 more work; the n/a item has no share.
    assert_eq!(ours["share"]["items"], 1);
    assert_eq!(ours["share"]["done"], 0.25);
    assert_eq!(ours["share"]["works"], 0.5);
    // Conditions: 3/3 and 1/2 done average to 75%, each item counting once.
    assert_eq!(v["areas"][0]["share"]["done"], 0.75);
    assert_eq!(v["areas"][0]["share"]["works"], 0.25);
    assert_eq!(v["totals"]["works"], 1);
    let speed = &ours["groups"][0]["items"][0];
    assert_eq!(speed["works"], true);
    assert_eq!(speed["share"]["total"], 4);
}

#[test]
fn a_report_without_works_rows_or_shares_has_no_such_fields() {
    covers(&[ids::report::format::WORKS, ids::report::format::SHARE]);
    let out = report(&Repo::valid(), &["--format", "json", "--depth", "3"]);
    assert!(
        !out.contains("\"works\"") && !out.contains("\"share\""),
        "{out}"
    );
    let md = report(&Repo::valid(), &["--format", "md"]);
    assert!(!md.contains("Average share"), "{md}");
    let svg = report(&Repo::valid(), &["--format", "svg"]);
    assert!(!svg.contains("#4f6472"), "{svg}");
}

#[test]
fn the_svg_report_draws_works_items_and_a_share_bar() {
    covers(&[
        ids::report::format::SVG,
        ids::report::format::WORKS,
        ids::report::format::SHARE,
    ]);
    let out = report(&with_extents(), &["--format", "svg"]);
    // The works segment and its legend entry, after the missing count it is part of.
    assert!(out.contains("fill=\"#4f6472\" clip-path"), "{out}");
    assert!(out.contains(">1 missing</text>"), "{out}");
    assert!(
        out.contains(">1 of them works via the old code</text>"),
        "{out}"
    );
    // A missing item that works another way is not a gap.
    assert!(!out.contains("gaps: ours.extra.speed"), "{out}");
    // A second bar for each area with shares.
    assert!(out.contains(">Conditions · native share</text>"), "{out}");
    assert!(out.contains(">75.0% native on average</text>"), "{out}");
    assert!(
        out.contains(">25.0% works via the old code</text>"),
        "{out}"
    );
    assert!(
        out.contains(">50.0% works via the old code</text>"),
        "{out}"
    );
    assert!(out.contains(">25.0% neither</text>"), "{out}");
    assert!(out.contains(">average over 2 items</text>"), "{out}");
    // Clip paths stay unique.
    let mut ids: Vec<&str> = out
        .match_indices("clipPath id=\"")
        .map(|(i, _)| &out[i + 14..i + 18])
        .collect();
    let n = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), n, "{out}");
    assert_eq!(out, report(&with_extents(), &["--format", "svg"]));
}

#[test]
fn the_markdown_and_terminal_reports_show_works_and_shares() {
    covers(&[
        ids::report::format::MD,
        ids::report::format::TERM,
        ids::report::format::WORKS,
        ids::report::format::SHARE,
    ]);
    let r = with_extents();
    let md = report(&r, &["--format", "md", "--depth", "3"]);
    assert!(
        md.contains("| n/a, deferred | Works via the old code |"),
        "{md}"
    );
    assert!(
        md.contains("| **ours** | 0/1 | 0 | 0 | 0 | 1 | 0 | 1 | 1 |"),
        "{md}"
    );
    assert!(
        md.contains("| Average share | Native | Works via the old code | Items |"),
        "{md}"
    );
    assert!(
        md.contains("| **Conditions** | 75.0% | 25.0% | 2 |"),
        "{md}"
    );
    assert!(
        md.contains("| missing (works via the old code), 1/4 native |"),
        "{md}"
    );
    let term = report(&r, &[]);
    assert!(
        term.contains("share: 25.0% native on average, 50.0% works via the old code (1 items)"),
        "{term}"
    );
    assert!(term.contains("1 works another way"), "{term}");
}
