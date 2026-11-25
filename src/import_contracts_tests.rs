// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::own::{covers, ids};

fn ev(groups: &[&str]) -> Evidence {
    Evidence {
        kind: "contracts".into(),
        path: "contracts.json".into(),
        key_column: None,
        status_column: None,
        implemented: Vec::new(),
        groups: groups.iter().map(ToString::to_string).collect(),
        works: Vec::new(),
        share_column: None,
    }
}

const HANDSHAKE: &str = r#"{"schema_version": 1, "contract": "ttwright-contracts",
"tool": {"name": "ttwright", "version": "0.5.0"},
"contracts": [
  {"name": "ttwright-result", "versions": [1, 2], "supports": {
    "packages": [{"name": "ttw", "versions": ["3.3", "3.4"]}],
    "features": [{"id": "asset_op:0", "description": "copy"}, {"id": "asset_op:2", "description": "patch"}]}},
  {"name": "ttwright-events", "versions": [1]},
  {"name": "ttwright-api", "versions": [1]}
]}"#;

#[test]
fn every_supported_feature_is_an_item_of_the_group() {
    covers(&[ids::refs::importers::CONTRACTS]);
    assert_eq!(
        contracts_rows(HANDSHAKE, &ev(&["ttw.features"])).unwrap(),
        vec![
            ("ttw.features.asset_op_0".to_string(), true),
            ("ttw.features.asset_op_2".to_string(), true),
        ]
    );
}

#[test]
fn a_contracts_importer_needs_exactly_one_group() {
    covers(&[ids::refs::importers::CONTRACTS]);
    assert!(contracts_rows(HANDSHAKE, &ev(&[])).is_err());
    assert!(contracts_rows(HANDSHAKE, &ev(&["a.b", "c.d"])).is_err());
}

#[test]
fn another_document_is_an_error() {
    covers(&[ids::refs::importers::CONTRACTS]);
    let e = ev(&["ttw.features"]);
    assert!(contracts_rows("x", &e).is_err());
    assert!(contracts_rows(r#"{"contract": "ttwright-result", "contracts": []}"#, &e).is_err());
    assert!(contracts_rows(r#"{"contract": "ttwright-contracts"}"#, &e).is_err());
}

#[test]
fn a_feature_without_an_id_is_an_error() {
    covers(&[ids::refs::importers::CONTRACTS]);
    let text = HANDSHAKE.replace(
        r#"{"id": "asset_op:2", "description": "patch"}"#,
        r#"{"description": "patch"}"#,
    );
    assert!(contracts_rows(&text, &ev(&["ttw.features"])).is_err());
}

#[test]
fn two_features_that_give_one_item_are_an_error() {
    covers(&[ids::refs::importers::CONTRACTS]);
    let text = HANDSHAKE.replace("asset_op:2", "asset_op-0");
    let err = contracts_rows(&text, &ev(&["ttw.features"])).unwrap_err();
    assert!(err.contains("both become"), "{err}");
}

#[test]
fn a_feature_listed_by_two_contracts_counts_once() {
    covers(&[ids::refs::importers::CONTRACTS]);
    let text = HANDSHAKE.replace(
        r#"{"name": "ttwright-events", "versions": [1]}"#,
        r#"{"name": "ttwright-events", "versions": [1], "supports": {"packages": [], "features": [{"id": "asset_op:0", "description": "copy"}]}}"#,
    );
    assert_eq!(
        contracts_rows(&text, &ev(&["ttw.features"])).unwrap().len(),
        2
    );
}

#[test]
fn an_empty_feature_id_is_an_error() {
    covers(&[ids::refs::importers::CONTRACTS]);
    let text = HANDSHAKE.replace("asset_op:2", "");
    assert!(
        contracts_rows(&text, &ev(&["ttw.features"]))
            .unwrap_err()
            .contains("empty name")
    );
}
