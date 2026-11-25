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
        works: Vec::new(),
        share_column: None,
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

fn tsv_ev() -> Evidence {
    Evidence {
        kind: "tsv".into(),
        status_column: Some("state".into()),
        implemented: vec!["implemented".into()],
        works: vec!["fallback".into()],
        share_column: Some("share".into()),
        ..ev(&[])
    }
}

const EXTENTS: &str = "id\tstate\tshare\n\
    a.b.one\timplemented\t3/0/3\n\
    a.b.two\tfallback\t1/2/4\n\
    a.b.three\tmissing\t\n\
    a.b.four\tmissing\t0/0/5\n";

#[test]
fn tsv_extents_read_works_states_and_shares() {
    covers(&[ids::check::evidence::WORKS_SHARE]);
    let x = tsv_extents(EXTENTS, &tsv_ev()).unwrap();
    let s = |done, works, total| Some(Share { done, works, total });
    assert_eq!(
        x["a.b.one"],
        Extent {
            works: false,
            share: s(3, 0, 3)
        }
    );
    assert_eq!(
        x["a.b.two"],
        Extent {
            works: true,
            share: s(1, 2, 4)
        }
    );
    assert_eq!(
        x["a.b.four"],
        Extent {
            works: false,
            share: s(0, 0, 5)
        }
    );
    // A row that neither works nor has a share says nothing more.
    assert!(!x.contains_key("a.b.three"));
    // The works state is not implemented.
    let rows = tsv_rows(EXTENTS, &tsv_ev()).unwrap();
    assert!(rows.contains(&("a.b.two".to_string(), false)));
}

#[test]
fn tsv_extents_are_empty_without_works_or_a_share_column() {
    covers(&[ids::check::evidence::WORKS_SHARE]);
    let plain = Evidence {
        works: Vec::new(),
        share_column: None,
        ..tsv_ev()
    };
    assert!(tsv_extents(EXTENTS, &plain).unwrap().is_empty());
}

#[test]
fn malformed_shares_and_overlapping_states_are_errors() {
    covers(&[ids::check::evidence::WORKS_SHARE]);
    let bad = |share: &str| format!("id\tstate\tshare\na.b.x\tmissing\t{share}\n");
    let max = u64::MAX;
    let wraps = format!("{max}/{max}/{max}");
    for share in ["1/2", "a/0/1", "1/0/0", "2/2/3", "1/1/1/1", wraps.as_str()] {
        assert!(tsv_extents(&bad(share), &tsv_ev()).is_err(), "{share}");
    }
    let both = Evidence {
        works: vec!["implemented".into()],
        ..tsv_ev()
    };
    assert!(tsv_extents(EXTENTS, &both).unwrap_err().contains("both"));
    let no_status = Evidence {
        status_column: None,
        ..tsv_ev()
    };
    assert!(tsv_extents(EXTENTS, &no_status).is_err());
    let twice = "id\tstate\tshare\na.b.x\tfallback\t\na.b.x\tfallback\t\n";
    assert!(tsv_extents(twice, &tsv_ev()).unwrap_err().contains("twice"));
}
