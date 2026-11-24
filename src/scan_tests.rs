// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::own::{covers, ids};

fn one(src: &str) -> Site {
    let (sites, _) = scan_source("tests/x.rs", src);
    assert_eq!(sites.len(), 1, "{sites:?}");
    sites.into_iter().next().unwrap()
}

#[test]
fn a_covers_call_in_a_test_names_the_test_and_the_dotted_ids() {
    covers(&[ids::check::tags::PATH]);
    let s = one(
        "#[test]\nfn a_b() {\n    covers(&[ids::area::group::ITEM_ONE, crate::own::ids::area::group::TWO]);\n}\n",
    );
    assert_eq!(s.ids, vec!["area.group.item_one", "area.group.two"]);
    assert_eq!(
        s.test,
        Some(TestRef {
            file: "tests/x.rs".into(),
            function: "a_b".into()
        })
    );
    assert!(s.is_test && !s.ignored && s.bad.is_empty());
    assert_eq!(s.line, 3);
}

#[test]
fn a_group_named_ids_keeps_its_name() {
    covers(&[ids::check::tags::PATH]);
    let s = one("#[test] fn t() { covers(&[ids::check::ids::SYNTAX]); }");
    assert_eq!(s.ids, vec!["check.ids.syntax"]);
}

#[test]
fn calls_inside_strings_and_comments_are_not_tags() {
    covers(&[ids::check::tags::PATH]);
    let src = r##"
#[test]
fn t() {
    let a = "covers(&[ids::a::b::C])";
    let b = r#"covers(&[ids::a::b::D]) " still raw"#;
    let c = b"covers(&[ids::a::b::E])";
    let d = '"';
    let e = '\'';
    // covers(&[ids::a::b::F])
    /* covers(&[ids::a::b::G]) /* nested */ */
    covers(&[ids::a::b::REAL]);
}
"##;
    assert_eq!(one(src).ids, vec!["a.b.real"]);
}

#[test]
fn lifetimes_do_not_start_a_character_literal() {
    covers(&[ids::check::tags::PATH]);
    let s = one("#[test]\nfn t<'a>(x: &'a str) { let _ = x; covers(&[ids::a::b::C]); }");
    assert_eq!(s.ids, vec!["a.b.c"]);
}

#[test]
fn a_test_attribute_with_a_path_counts_as_a_test() {
    covers(&[ids::check::tags::OUTSIDE_TEST]);
    assert!(one("#[tokio::test]\nasync fn t() { covers(&[ids::a::b::C]); }").is_test);
    assert!(one("#[rstest]\nfn t() { covers(&[ids::a::b::C]); }").is_test);
}

#[test]
fn a_helper_function_is_not_a_test() {
    covers(&[ids::check::tags::OUTSIDE_TEST]);
    let s = one("#[derive(Debug)]\nstruct S;\nfn helper() { covers(&[ids::a::b::C]); }");
    assert!(!s.is_test);
    assert_eq!(s.test.unwrap().function, "helper");
}

#[test]
fn a_call_after_a_nested_function_belongs_to_the_outer_test() {
    covers(&[ids::check::tags::OUTSIDE_TEST]);
    let s = one(
        "#[test]\nfn outer() {\n    fn inner() {}\n    inner();\n    covers(&[ids::a::b::C]);\n}\n",
    );
    assert_eq!(s.test.unwrap().function, "outer");
    assert!(s.is_test);
}

#[test]
fn an_ignored_test_is_marked_ignored() {
    covers(&[ids::check::tags::IGNORED]);
    assert!(one("#[test]\n#[ignore]\nfn t() { covers(&[ids::a::b::C]); }").ignored);
    assert!(
        one("#[test]\n#[cfg_attr(windows, ignore)]\nfn t() { covers(&[ids::a::b::C]); }").ignored
    );
    assert!(!one("#[test]\n#[should_panic]\nfn t() { covers(&[ids::a::b::C]); }").ignored);
}

#[test]
fn the_definition_of_covers_is_not_a_call() {
    covers(&[ids::check::tags::PATH]);
    let (sites, _) = scan_source("src/x.rs", "pub fn covers(ids: &[Id]) {}\n");
    assert_eq!(sites, Vec::new());
}

#[test]
fn a_lower_case_constant_or_upper_case_module_is_a_bad_path() {
    covers(&[ids::check::tags::PATH]);
    assert_eq!(
        one("#[test] fn t() { covers(&[ids::a::b::c]); }").bad,
        vec!["ids::a::b::c"]
    );
    assert_eq!(
        one("#[test] fn t() { covers(&[ids::A::b::C]); }").bad,
        vec!["ids::A::b::C"]
    );
    assert_eq!(
        one("#[test] fn t() { covers(&[ids::a::C]); }").bad,
        vec!["ids::a::C"]
    );
}

#[test]
fn a_covers_table_comment_near_the_test_is_found() {
    covers(&[ids::check::tags::BULK]);
    assert!(
        one("// covers-table: one row each\n#[test]\nfn t() { covers(&[ids::a::b::C]); }").table
    );
    assert!(
        !one("// covers-table: far away\n\n\n\n\n#[test]\nfn t() { covers(&[ids::a::b::C]); }")
            .table
    );
}

#[test]
fn test_functions_are_listed_with_their_ignore_state() {
    covers(&[ids::check::tags::DATA]);
    let (_, tests) = scan_source(
        "tests/x.rs",
        "#[test]\nfn a() {}\n#[test]\n#[ignore]\nfn b() {}\nfn c() {}\n",
    );
    let names: Vec<(String, bool)> = tests
        .into_iter()
        .map(|t| (t.test.function, t.ignored))
        .collect();
    assert_eq!(names, vec![("a".into(), false), ("b".into(), true)]);
}

#[test]
fn a_raw_identifier_is_read_without_its_prefix() {
    covers(&[ids::check::tags::PATH]);
    assert_eq!(
        one("#[test] fn r#type() { covers(&[ids::a::b::C]); }")
            .test
            .unwrap()
            .function,
        "type"
    );
}

#[test]
fn the_glob_matches_one_segment_per_star() {
    covers(&[ids::check::tags::DATA]);
    assert!(wildcard("*.toml", "covers.toml"));
    assert!(wildcard("case_*_x", "case_12_x"));
    assert!(!wildcard("*.toml", "covers.tom"));
    assert!(wildcard("*", ""));
}
