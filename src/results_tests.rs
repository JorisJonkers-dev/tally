// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::own::{covers, ids};

#[test]
fn a_nextest_report_reads_into_cases_with_their_binary() {
    covers(&[ids::results::junit::PASS]);
    let xml = r#"<testsuites><testsuite name="pkg::golden"><testcase name="case_a" classname="pkg::golden"/>
<testcase name="case_b" classname="pkg::golden"><failure>no</failure></testcase>
<testcase name="case_c"><skipped message="x"/></testcase>
<testcase name="case_d"><error/></testcase>
<testcase name="case_e"><rerunFailure/></testcase></testsuite></testsuites>"#;
    let cases = parse_junit(xml).unwrap();
    let got: Vec<(&str, &str, Outcome)> = cases
        .iter()
        .map(|c| (c.suite.as_str(), c.name.as_str(), c.outcome))
        .collect();
    assert_eq!(
        got,
        vec![
            ("pkg::golden", "case_a", Outcome::Pass),
            ("pkg::golden", "case_b", Outcome::Fail),
            ("pkg::golden", "case_c", Outcome::Skip),
            ("pkg::golden", "case_d", Outcome::Fail),
            ("pkg::golden", "case_e", Outcome::Fail),
        ]
    );
}

#[test]
fn escaped_attribute_values_are_decoded() {
    covers(&[ids::results::junit::PASS]);
    let cases = parse_junit(
        "<testsuite name='a&amp;b'><testcase name=\"x&lt;y&gt;&quot;&apos;\"/></testsuite>",
    )
    .unwrap();
    assert_eq!(cases[0].suite, "a&b");
    assert_eq!(cases[0].name, "x<y>\"'");
}

#[test]
fn comments_and_declarations_are_skipped() {
    covers(&[ids::results::junit::PASS]);
    let cases = parse_junit(
        "<?xml version=\"1.0\"?><!-- <testcase name=\"no\"/> --><testsuites></testsuites>",
    )
    .unwrap();
    assert_eq!(cases, Vec::new());
}

#[test]
fn a_case_without_a_name_or_an_unclosed_tag_is_an_error() {
    covers(&[ids::results::junit::FAILURE]);
    assert!(parse_junit("<testsuite><testcase/></testsuite>").is_err());
    assert!(parse_junit("<testsuite name=\"x\"").is_err());
}

#[test]
fn the_os_is_read_from_the_file_name() {
    covers(&[ids::results::junit::OS]);
    assert_eq!(os_from_name("out/junit-ubuntu-latest.xml"), Some("linux"));
    assert_eq!(os_from_name("junit-macos.xml"), Some("macos"));
    assert_eq!(os_from_name("x/darwin.xml"), Some("macos"));
    assert_eq!(os_from_name("Windows.xml"), Some("windows"));
    assert_eq!(os_from_name("linux/report.xml"), None);
    assert_eq!(os_from_name("linux-windows.xml"), None);
}

#[test]
fn a_source_test_matches_the_library_and_program_binaries() {
    covers(&[ids::results::junit::PASS]);
    let run = |suite: &str, name: &str, outcome| Results {
        schema: SCHEMA.into(),
        runs: vec![Run {
            os: "linux".into(),
            cases: vec![Case {
                suite: suite.into(),
                name: name.into(),
                outcome,
            }],
        }],
    };
    let t = TestRef {
        file: "src/cli.rs".into(),
        function: "parses".into(),
    };
    assert_eq!(
        outcome(
            &run("pkg", "cli::tests::parses", Outcome::Pass),
            "pkg",
            &t,
            "linux"
        ),
        Some(true)
    );
    assert_eq!(
        outcome(
            &run("pkg::bin/pkg", "tests::parses", Outcome::Fail),
            "pkg",
            &t,
            "linux"
        ),
        Some(false)
    );
    assert_eq!(
        outcome(
            &run("pkg::other", "parses", Outcome::Pass),
            "pkg",
            &t,
            "linux"
        ),
        None
    );
    assert_eq!(
        outcome(
            &run("pkg", "parses_more", Outcome::Pass),
            "pkg",
            &t,
            "linux"
        ),
        None
    );
    assert_eq!(
        outcome(&run("pkg", "parses", Outcome::Pass), "pkg", &t, "windows"),
        None
    );
}

const CARGO_TEST: &str = "\u{1b}[1m\u{1b}[92m     Running\u{1b}[0m unittests src/lib.rs (target/debug/build/plugin-host/0123abcd/out/plugin_host-0123abcd)

running 3 tests
test tests::loads_a_plugin ... ok
test tests::refuses_a_bad_one ... FAILED
test tests::slow ... ignored, needs a game
test result: FAILED. 1 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out

     Running tests\\saves.rs (target\\debug\\build\\host/89ab\\out\\saves-89ab.exe)
test a_save_round_trips ... ok
     Running unittests src/main.rs (target/debug/deps/nv_viewer-77aa)
test tests::opens ... ok
     Running tests/old.rs (target/debug/deps/old-1234)
test old_layout ... ok
   Doc-tests plugin_host
test crates/plugin-host/src/lib.rs - load (line 3) ... ok
test tests::noisy ... some output
";

#[test]
fn cargo_test_output_reads_into_cases_of_each_binary() {
    covers(&[ids::results::libtest::CASES, ids::results::libtest::SUITES]);
    let cases = parse_libtest(CARGO_TEST).unwrap();
    let got: Vec<(&str, &str, Outcome)> = cases
        .iter()
        .map(|c| (c.suite.as_str(), c.name.as_str(), c.outcome))
        .collect();
    assert_eq!(
        got,
        vec![
            ("plugin-host", "tests::loads_a_plugin", Outcome::Pass),
            ("plugin-host", "tests::refuses_a_bad_one", Outcome::Fail),
            ("plugin-host", "tests::slow", Outcome::Skip),
            ("host::saves", "a_save_round_trips", Outcome::Pass),
            ("nv_viewer::bin/nv_viewer", "tests::opens", Outcome::Pass),
            ("::old", "old_layout", Outcome::Pass),
        ]
    );
}

#[test]
fn doc_tests_and_cases_without_a_result_never_count() {
    covers(&[ids::results::libtest::PARTIAL]);
    let cases = parse_libtest(CARGO_TEST).unwrap();
    assert!(cases.iter().all(|c| !c.name.contains("line 3")));
    assert!(cases.iter().all(|c| c.name != "tests::noisy"));
    assert!(parse_libtest("test a ... ok\n").is_err());
    assert_eq!(
        parse_libtest(
            "     Running benches/b.rs (target/release/deps/b-1)\ntest b ... bench: 1 ns/iter\n"
        )
        .unwrap(),
        Vec::new()
    );
}

#[test]
fn a_workspace_test_matches_its_own_package_only() {
    covers(&[
        ids::results::workspace::PACKAGE,
        ids::results::workspace::UNNAMED_PACKAGE,
    ]);
    let res = Results {
        schema: SCHEMA.into(),
        runs: vec![Run {
            os: "windows".into(),
            cases: parse_libtest(CARGO_TEST).unwrap(),
        }],
    };
    let index = Index::new(&res);
    // `plugin-host` and `plugin_host` are one package.
    assert_eq!(
        index.outcome("plugin_host", "src/lib.rs", "loads_a_plugin", "windows"),
        Some(true)
    );
    assert_eq!(
        index.outcome("plugin-host", "src/x.rs", "refuses_a_bad_one", "windows"),
        Some(false)
    );
    assert_eq!(
        index.outcome("plugin-host", "src/x.rs", "slow", "windows"),
        Some(false)
    );
    assert_eq!(
        index.outcome("host", "src/lib.rs", "loads_a_plugin", "windows"),
        None
    );
    assert_eq!(
        index.outcome("host", "tests/saves.rs", "a_save_round_trips", "windows"),
        Some(true)
    );
    assert_eq!(
        index.outcome("jip", "tests/saves.rs", "a_save_round_trips", "windows"),
        None
    );
    assert_eq!(
        index.outcome("nv-viewer", "src/main.rs", "opens", "windows"),
        Some(true)
    );
    // The older layout names no package for an integration test: it could be any package's, so it never counts.
    assert_eq!(
        index.outcome("anything", "tests/old.rs", "old_layout", "windows"),
        None
    );
    assert_eq!(
        index.outcome("host", "tests/saves.rs", "a_save_round_trips", "linux"),
        None
    );
}
