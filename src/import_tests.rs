// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::own::{covers, ids};

fn ev(groups: &[&str]) -> Evidence {
    Evidence {
        kind: "ledger".into(),
        path: "ledger.tsv".into(),
        key_column: None,
        status_column: None,
        implemented: Vec::new(),
        groups: groups.iter().map(ToString::to_string).collect(),
    }
}

const LEDGER: &str = "address\tsize\tname\tsubsystem\tunit\tstatus\trust\ttyped\n\
    00401000\t21\tFoo\tcore\tu\ttranslated\tcrates/engine/src/a.rs:1\t\n\
    00401020\t10\t\tcore\tu\topen\t\t\n\
    00401030\t21\tBar\tcore\tu\tstatic init (table)\t\t\n\
    00401050\t35\tBaz\tcore\tu\ttraced\t\t\n";

#[test]
fn a_ledger_key_is_f_and_eight_hex_digits() {
    covers(&[ids::refs::importers::LEDGER]);
    assert_eq!(ledger_key("00401000").unwrap(), "f00401000");
    assert_eq!(ledger_key("0x401ABC").unwrap(), "f00401abc");
    assert!(ledger_key("").is_err());
    assert!(ledger_key("g0401000").is_err());
    assert!(ledger_key("100401000").is_err());
}

#[test]
fn ledger_rows_mark_translated_and_table_functions_implemented() {
    covers(&[ids::refs::importers::LEDGER]);
    let rows = ledger_rows(LEDGER, &ev(&["engine.fns"])).unwrap();
    assert_eq!(
        rows,
        vec![
            ("engine.fns.f00401000".to_string(), true),
            ("engine.fns.f00401020".to_string(), false),
            ("engine.fns.f00401030".to_string(), true),
            ("engine.fns.f00401050".to_string(), false),
        ]
    );
}

#[test]
fn ledger_rows_take_the_states_given() {
    covers(&[ids::refs::importers::LEDGER]);
    let mut e = ev(&["engine.fns"]);
    e.implemented = vec!["traced".into()];
    let rows = ledger_rows(LEDGER, &e).unwrap();
    assert_eq!(rows.iter().filter(|(_, d)| *d).count(), 1);
}

#[test]
fn a_ledger_importer_needs_exactly_one_group() {
    covers(&[ids::refs::importers::LEDGER]);
    assert!(ledger_rows(LEDGER, &ev(&[])).is_err());
    assert!(ledger_rows(LEDGER, &ev(&["a.b", "c.d"])).is_err());
}

#[test]
fn a_ledger_without_a_status_column_is_an_error() {
    covers(&[ids::refs::importers::LEDGER]);
    let err = ledger_rows("address\tname\n00401000\tFoo\n", &ev(&["engine.fns"])).unwrap_err();
    assert!(err.contains("status"), "{err}");
}

#[test]
fn the_ledger_list_skips_comments_and_names_unnamed_functions_by_address() {
    covers(&[ids::refs::importers::LEDGER]);
    let map = "# FalloutNV.exe\n# names\naddress\tsize\tname\n00401020\t10\t\n00401000\t21\tFoo\n";
    assert_eq!(
        ledger_list(map).unwrap(),
        "key\ttitle\nf00401000\tFoo\nf00401020\t0x00401020\n"
    );
}

#[test]
fn the_ledger_list_refuses_an_address_listed_twice() {
    covers(&[ids::refs::importers::LEDGER]);
    let map = "address\tname\n00401000\tFoo\n401000\tBar\n";
    assert!(ledger_list(map).unwrap_err().contains("twice"));
}

#[test]
fn a_ledger_listing_an_address_twice_is_an_error() {
    covers(&[ids::refs::importers::LEDGER]);
    let text = "address\tstatus\n00401000\topen\n0x401000\ttranslated\n";
    let err = ledger_rows(text, &ev(&["engine.fns"])).unwrap_err();
    assert!(err.contains("twice"), "{err}");
}
