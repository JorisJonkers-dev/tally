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
