// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::own::{covers, ids};

fn ev(groups: &[&str]) -> Evidence {
    Evidence {
        kind: "capabilities".into(),
        path: ".fork/capabilities.json".into(),
        key_column: None,
        status_column: None,
        implemented: Vec::new(),
        groups: groups.iter().map(ToString::to_string).collect(),
    }
}

const CAPS: &str = r#"{"schema_version": 1, "extenders": [
  {"id": "xnvse", "commands": {"implemented": 2, "implemented_names": ["GetNVSEVersion", "Return"]}},
  {"id": "jip-ln", "commands": {"implemented": 1, "implemented_names": ["GetLNVersion"]}},
  {"id": "stewie-tweaks", "commands": {"implemented": 0, "implemented_names": []}}
]}"#;

#[test]
fn a_segment_is_lower_case_with_underscores_and_never_a_keyword() {
    covers(&[ids::refs::importers::CAPABILITIES]);
    assert_eq!(segment("GetNVSEVersion"), "getnvseversion");
    assert_eq!(segment("jip-ln"), "jip_ln");
    assert_eq!(segment("asset_op:3"), "asset_op_3");
    assert_eq!(segment("Return"), "return_");
    assert_eq!(segment("3DText"), "x3dtext");
    assert_eq!(segment("_x"), "x_x");
}

#[test]
fn each_extender_s_commands_are_items_of_its_group() {
    covers(&[ids::refs::importers::CAPABILITIES]);
    let rows =
        capabilities_rows(CAPS, &ev(&["ext.xnvse", "ext.jip_ln", "ext.stewie_tweaks"])).unwrap();
    assert_eq!(
        rows,
        vec![
            ("ext.xnvse.getnvseversion".to_string(), true),
            ("ext.xnvse.return_".to_string(), true),
            ("ext.jip_ln.getlnversion".to_string(), true),
        ]
    );
}

#[test]
fn extenders_without_a_group_are_left_out() {
    covers(&[ids::refs::importers::CAPABILITIES]);
    let rows = capabilities_rows(CAPS, &ev(&["ext.jip_ln"])).unwrap();
    assert_eq!(rows, vec![("ext.jip_ln.getlnversion".to_string(), true)]);
}

#[test]
fn a_capabilities_importer_needs_its_groups() {
    covers(&[ids::refs::importers::CAPABILITIES]);
    assert!(capabilities_rows(CAPS, &ev(&[])).is_err());
    let err = capabilities_rows(CAPS, &ev(&["ext.showoff"])).unwrap_err();
    assert!(err.contains("matches no extender"), "{err}");
}

#[test]
fn a_count_that_disagrees_with_the_list_is_an_error() {
    covers(&[ids::refs::importers::CAPABILITIES]);
    let text = CAPS.replace("\"implemented\": 2", "\"implemented\": 3");
    let err = capabilities_rows(&text, &ev(&["ext.xnvse"])).unwrap_err();
    assert!(err.contains("says 3"), "{err}");
}

#[test]
fn two_commands_that_give_one_item_are_an_error() {
    covers(&[ids::refs::importers::CAPABILITIES]);
    let text = CAPS.replace(
        "\"implemented\": 2, \"implemented_names\": [\"GetNVSEVersion\", \"Return\"]",
        "\"implemented\": 2, \"implemented_names\": [\"GetX\", \"getx\"]",
    );
    let err = capabilities_rows(&text, &ev(&["ext.xnvse"])).unwrap_err();
    assert!(err.contains("both become"), "{err}");
}

#[test]
fn a_file_that_is_not_the_capabilities_json_is_an_error() {
    covers(&[ids::refs::importers::CAPABILITIES]);
    assert!(capabilities_rows("x", &ev(&["ext.xnvse"])).is_err());
    assert!(capabilities_rows("{}", &ev(&["ext.xnvse"])).is_err());
    assert!(capabilities_rows(r#"{"extenders":[{"id":"xnvse"}]}"#, &ev(&["ext.xnvse"])).is_err());
}

#[test]
fn a_command_listed_twice_counts_once() {
    covers(&[ids::refs::importers::CAPABILITIES]);
    let text = CAPS
        .replace("[\"GetLNVersion\"]", "[\"GetLNVersion\", \"GetLNVersion\"]")
        .replace("\"implemented\": 1,", "\"implemented\": 2,");
    let rows = capabilities_rows(&text, &ev(&["ext.jip_ln"])).unwrap();
    assert_eq!(rows, vec![("ext.jip_ln.getlnversion".to_string(), true)]);
}
