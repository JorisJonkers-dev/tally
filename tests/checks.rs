// SPDX-License-Identifier: GPL-3.0-only
//! Every check of `tally check`, each shown to pass on the valid fixture and to fail on one bad input.

mod common;

use std::fmt::Write as _;

use common::{LIST, Repo, covers, ids, sha, sources};

#[test]
fn the_valid_fixture_passes_every_check() {
    covers(&[ids::cmd::cli::CHECK]);
    let out = Repo::valid().passes(&[]);
    assert!(out.contains("0 errors, 0 warnings"), "{out}");
}

// Ids.

#[test]
fn a_group_id_with_one_segment_is_refused() {
    covers(&[ids::check::ids::SYNTAX]);
    let r = Repo::valid();
    r.edit(
        "catalogue/cond/func.toml",
        "id = \"cond.func\"",
        "id = \"cond\"",
    );
    r.fails_with("id-syntax");
}

#[test]
fn an_item_id_with_capitals_is_refused() {
    covers(&[ids::check::ids::SYNTAX]);
    let r = Repo::valid();
    r.edit(
        "catalogue/cond/func.toml",
        "id = \"active\"",
        "id = \"Active\"",
    );
    r.fails_with("id-syntax");
}

#[test]
fn an_item_id_that_is_a_rust_keyword_is_refused() {
    covers(&[ids::check::ids::SYNTAX]);
    let r = Repo::valid();
    r.edit(
        "catalogue/cond/func.toml",
        "id = \"active\"",
        "id = \"match\"",
    );
    r.fails_with("id-syntax");
}

#[test]
fn an_invalid_alias_is_refused() {
    covers(&[ids::check::ids::SYNTAX]);
    let r = Repo::valid();
    r.edit(
        "catalogue/cond/func.toml",
        "id = \"active\"\n",
        "id = \"active\"\naliases = [\"Old.Name\"]\n",
    );
    r.fails_with("id-syntax");
}

#[test]
fn a_group_in_the_wrong_file_is_refused() {
    covers(&[ids::check::ids::PATH]);
    let r = Repo::valid();
    let text = r.read("catalogue/cond/func.toml");
    r.remove("catalogue/cond/func.toml");
    r.write("catalogue/cond/functions.toml", &text);
    r.fails_with("id-path");
}

#[test]
fn an_item_defined_twice_is_refused() {
    covers(&[ids::check::ids::UNIQUE]);
    let r = Repo::valid();
    r.edit(
        "catalogue/ours/extra.toml",
        "id = \"speed\"",
        "id = \"other_game\"",
    );
    r.fails_with("id-unique");
}

#[test]
fn an_alias_that_is_also_an_item_is_refused() {
    covers(&[ids::check::ids::UNIQUE]);
    let r = Repo::valid();
    r.edit(
        "catalogue/ours/extra.toml",
        "id = \"speed\"\n",
        "id = \"speed\"\naliases = [\"cond.func.file\"]\n",
    );
    r.fails_with("id-unique");
}

#[test]
fn a_group_defined_twice_is_refused() {
    covers(&[ids::check::ids::UNIQUE]);
    let r = Repo::valid();
    r.write(
        "catalogue/cond/func/x.toml",
        "[group]\nid = \"cond.func\"\ntitle = \"x\"\norigin = \"ours\"\n\n[[item]]\nid = \"y\"\n",
    );
    r.fails_with("id-unique");
}

#[test]
fn an_id_that_is_both_a_group_and_an_item_is_refused() {
    covers(&[ids::check::ids::UNIQUE]);
    let r = Repo::valid();
    r.write("catalogue/ours/cond.toml", "[group]\nid = \"ours.cond\"\ntitle = \"x\"\norigin = \"ours\"\n\n[[item]]\nid = \"func\"\n");
    r.edit(
        "catalogue/ours/extra.toml",
        "id = \"speed\"\n",
        "id = \"speed\"\naliases = [\"ours.cond\"]\n",
    );
    r.fails_with("id-unique");
}

#[test]
fn a_group_without_items_is_refused() {
    covers(&[ids::check::ids::GROUP_EMPTY]);
    let r = Repo::valid();
    r.write(
        "catalogue/ours/empty.toml",
        "[group]\nid = \"ours.empty\"\ntitle = \"Empty\"\norigin = \"ours\"\n",
    );
    r.fails_with("group-empty");
}

#[test]
fn an_item_that_is_not_applicable_without_a_reason_is_refused() {
    covers(&[ids::check::ids::REASONS]);
    let r = Repo::valid();
    r.edit(
        "catalogue/ours/extra.toml",
        "reason = \"New Vegas only\"\n",
        "",
    );
    r.fails_with("status-reason");
}

#[test]
fn an_empty_gated_reason_is_refused() {
    covers(&[ids::check::ids::REASONS]);
    let r = Repo::valid();
    r.edit(
        "catalogue/cond/func.toml",
        "status = \"partial\"\n",
        "status = \"partial\"\ngated = \" \"\n",
    );
    r.fails_with("status-reason");
}

#[test]
fn an_empty_retired_reason_is_refused() {
    covers(&[ids::check::ids::REASONS]);
    let r = Repo::valid();
    r.edit(
        "catalogue/ours/extra.toml",
        "title = \"Fast sort\"\n",
        "title = \"Fast sort\"\nretired = \"\"\n",
    );
    r.fails_with("status-reason");
}

#[test]
fn a_group_file_next_to_tally_toml_is_refused() {
    covers(&[ids::check::ids::TOP_LEVEL]);
    let r = Repo::valid();
    r.write("catalogue/stray.toml", "[group]\nid = \"stray.x\"\n");
    r.fails_with("parse");
}

#[test]
fn a_group_file_that_does_not_parse_is_reported_and_the_rest_still_checked() {
    covers(&[ids::check::ids::PARSE]);
    let r = Repo::valid();
    r.edit(
        "catalogue/ours/extra.toml",
        "origin = \"ours\"",
        "origin = \"elsewhere\"",
    );
    r.edit("catalogue/sources.toml", "tag = \"1.2.3\"", "tag = \"\"");
    let out = r.fails_with("parse");
    assert!(out.contains("error[pin-tag]"), "{out}");
}

#[test]
fn an_unknown_key_in_a_group_file_is_refused() {
    covers(&[ids::check::ids::PARSE]);
    let r = Repo::valid();
    r.edit(
        "catalogue/ours/extra.toml",
        "title = \"Fast sort\"",
        "title = \"Fast sort\"\nstatuss = \"missing\"",
    );
    r.fails_with("parse");
}

// Pins.

#[test]
fn a_short_commit_pin_is_refused() {
    covers(&[ids::check::pins::COMMIT]);
    let r = Repo::valid();
    r.edit(
        "catalogue/sources.toml",
        "0123456789abcdef0123456789abcdef01234567",
        "0123456",
    );
    r.fails_with("pin-commit");
}

#[test]
fn a_source_without_a_tag_is_refused() {
    covers(&[ids::check::pins::COMMIT]);
    let r = Repo::valid();
    r.edit("catalogue/sources.toml", "tag = \"1.2.3\"", "tag = \" \"");
    r.fails_with("pin-tag");
}

#[test]
fn a_read_date_that_is_not_a_date_is_refused() {
    covers(&[ids::check::pins::COMMIT]);
    let r = Repo::valid();
    r.edit(
        "catalogue/sources.toml",
        "read = \"2026-11-24\"",
        "read = \"last week\"",
    );
    r.fails_with("pin-read");
}

#[test]
fn a_repository_that_is_not_https_is_refused() {
    covers(&[ids::check::pins::COMMIT]);
    let r = Repo::valid();
    r.edit(
        "catalogue/sources.toml",
        "https://github.com/example/upstream",
        "git@github.com:example/upstream",
    );
    r.fails_with("pin-repo");
}

#[test]
fn a_list_that_changed_after_it_was_pinned_is_refused() {
    covers(&[ids::check::pins::SHA256]);
    let r = Repo::valid();
    r.write(
        "catalogue/ref/up.tsv",
        &format!("{LIST}many\tmany(regex)\n"),
    );
    r.fails_with("pin-sha256");
}

#[test]
fn a_source_without_upstream_files_is_refused() {
    covers(&[ids::check::pins::UPSTREAM]);
    let r = Repo::valid();
    let text = r.read("catalogue/sources.toml");
    let cut = text.split("[source.upstream]").next().unwrap().to_string();
    r.write("catalogue/sources.toml", &cut);
    r.fails_with("pin-upstream");
}

#[test]
fn an_upstream_file_without_a_valid_hash_is_refused() {
    covers(&[ids::check::pins::UPSTREAM]);
    let r = Repo::valid();
    r.edit(
        "catalogue/sources.toml",
        "\"docs/conditions.md\" = \"",
        "\"docs/conditions.md\" = \"x",
    );
    r.fails_with("pin-upstream");
}

#[test]
fn a_planned_extractor_is_refused_with_its_issue() {
    covers(&[ids::check::pins::EXTRACTOR, ids::refs::importers::TSV]);
    let r = Repo::valid();
    r.edit(
        "catalogue/sources.toml",
        "extractor = \"manual\"",
        "extractor = \"contracts\"",
    );
    let out = r.fails_with("extractor-planned");
    assert!(out.contains("tally/issues/5"), "{out}");
}

#[test]
fn a_ledger_extractor_without_its_table_is_refused() {
    covers(&[ids::check::pins::EXTRACTOR, ids::refs::importers::LEDGER]);
    let r = Repo::valid();
    r.edit(
        "catalogue/sources.toml",
        "extractor = \"manual\"",
        "extractor = \"ledger\"",
    );
    r.fails_with("extractor-ledger");
}

#[test]
fn a_ledger_extractor_reading_an_unpinned_file_is_refused() {
    covers(&[ids::check::pins::EXTRACTOR, ids::refs::importers::LEDGER]);
    let r = Repo::valid();
    r.edit(
        "catalogue/sources.toml",
        "extractor = \"manual\"",
        "extractor = \"ledger\"",
    );
    let text = r.read("catalogue/sources.toml");
    r.write(
        "catalogue/sources.toml",
        &format!("{text}\n[source.ledger]\nfrom = \"research/engine_map.tsv\"\n"),
    );
    r.fails_with("extractor-ledger");
}

#[test]
fn an_upstream_path_that_leaves_the_cache_is_refused() {
    covers(&[ids::check::pins::UPSTREAM]);
    let r = Repo::valid();
    r.edit(
        "catalogue/sources.toml",
        "\"docs/conditions.md\" = ",
        "\"../../evil.md\" = ",
    );
    r.fails_with("path-escape");
}

#[test]
fn a_ledger_extractor_reading_a_pinned_file_passes() {
    covers(&[ids::check::pins::EXTRACTOR, ids::refs::importers::LEDGER]);
    let r = Repo::valid();
    r.edit(
        "catalogue/sources.toml",
        "extractor = \"manual\"",
        "extractor = \"ledger\"",
    );
    let text = r.read("catalogue/sources.toml");
    r.write(
        "catalogue/sources.toml",
        &format!("{text}\n[source.ledger]\nfrom = \"docs/conditions.md\"\n"),
    );
    r.passes(&[]);
}

#[test]
fn an_unknown_extractor_is_refused() {
    covers(&[ids::check::pins::EXTRACTOR]);
    let r = Repo::valid();
    r.edit(
        "catalogue/sources.toml",
        "extractor = \"manual\"",
        "extractor = \"magic\"",
    );
    r.fails_with("extractor-unknown");
}

#[test]
fn a_tsv_extractor_without_its_table_is_refused() {
    covers(&[ids::check::pins::EXTRACTOR]);
    let r = Repo::valid();
    r.edit(
        "catalogue/sources.toml",
        "extractor = \"manual\"",
        "extractor = \"tsv\"",
    );
    r.fails_with("extractor-tsv");
}

#[test]
fn a_tsv_extractor_reading_an_unpinned_file_is_refused() {
    covers(&[ids::check::pins::EXTRACTOR]);
    let r = Repo::valid();
    r.edit(
        "catalogue/sources.toml",
        "extractor = \"manual\"",
        "extractor = \"tsv\"",
    );
    let text = r.read("catalogue/sources.toml");
    r.write(
        "catalogue/sources.toml",
        &format!(
            "{text}\n[source.tsv]\nfrom = \"other.tsv\"\nkey_column = \"k\"\ntitle_column = \"t\"\n"
        ),
    );
    r.fails_with("extractor-tsv");
}

#[test]
fn a_missing_list_is_refused() {
    covers(&[ids::check::pins::LIST]);
    let r = Repo::valid();
    r.remove("catalogue/ref/up.tsv");
    r.fails_with("pin-list");
}

#[test]
fn a_list_without_its_header_is_refused() {
    covers(&[ids::check::pins::LIST]);
    let r = Repo::valid();
    let list = "file\tfile(path)\n";
    r.write("catalogue/ref/up.tsv", list);
    r.write("catalogue/sources.toml", &sources(list));
    r.fails_with("pin-list");
}

#[test]
fn a_list_key_that_appears_twice_is_refused() {
    covers(&[ids::check::pins::UNIQUE]);
    let r = Repo::valid();
    let list = format!("{LIST}file\tfile again\n");
    r.write("catalogue/ref/up.tsv", &list);
    r.write("catalogue/sources.toml", &sources(&list));
    r.fails_with("ref-dup");
}

#[test]
fn a_source_id_used_twice_is_refused() {
    covers(&[ids::check::pins::UNIQUE]);
    let r = Repo::valid();
    let text = sources(LIST);
    r.write("catalogue/sources.toml", &format!("{text}\n{text}"));
    r.fails_with("source-unique");
}

// References.

#[test]
fn an_item_reference_to_a_row_the_list_lacks_is_refused() {
    covers(&[ids::check::refs::UNKNOWN]);
    let r = Repo::valid();
    r.edit(
        "catalogue/cond/func.toml",
        "ref = [\"up:active\"]",
        "ref = [\"up:active\", \"up:many\"]",
    );
    r.fails_with("ref-unknown");
}

#[test]
fn an_item_reference_to_an_unknown_source_is_refused() {
    covers(&[ids::check::refs::UNKNOWN]);
    let r = Repo::valid();
    r.edit(
        "catalogue/cond/func.toml",
        "ref = [\"up:active\"]",
        "ref = [\"up:active\", \"down:active\"]",
    );
    r.fails_with("source-unknown");
}

#[test]
fn a_group_reference_to_an_unknown_source_is_refused() {
    covers(&[ids::check::refs::UNKNOWN]);
    let r = Repo::valid();
    r.edit(
        "catalogue/cond/func.toml",
        "reference = \"up\"",
        "reference = \"down\"",
    );
    r.fails_with("source-unknown");
}

#[test]
fn an_item_reference_without_a_source_is_refused() {
    covers(&[ids::check::refs::UNKNOWN]);
    let r = Repo::valid();
    r.edit(
        "catalogue/cond/func.toml",
        "ref = [\"up:active\"]",
        "ref = [\"active\"]",
    );
    r.fails_with("ref-syntax");
}

#[test]
fn a_reference_row_without_an_item_is_refused() {
    covers(&[ids::check::refs::UNCLAIMED]);
    let r = Repo::valid();
    r.edit("catalogue/cond/func.toml", "ref = [\"up:active\"]\n", "");
    let out = r.fails_with("ref-unclaimed");
    assert!(out.contains("`active`"), "{out}");
}

// Evidence.

#[test]
fn evidence_naming_a_missing_file_is_refused() {
    covers(&[ids::check::evidence::FILES]);
    let r = Repo::valid();
    r.edit(
        "catalogue/cond/func.toml",
        "src/lib.rs#pub fn file",
        "src/gone.rs",
    );
    r.fails_with("evidence-file");
}

#[test]
fn evidence_naming_text_the_file_lacks_is_refused() {
    covers(&[ids::check::evidence::FILES]);
    let r = Repo::valid();
    r.edit("src/lib.rs", "pub fn file() {}", "pub fn find() {}");
    r.fails_with("evidence-text");
}

#[test]
fn evidence_on_a_missing_item_is_refused() {
    covers(&[ids::check::evidence::STATE]);
    let r = Repo::valid();
    r.edit(
        "catalogue/ours/extra.toml",
        "title = \"Fast sort\"\n",
        "title = \"Fast sort\"\nevidence = [\"src/lib.rs\"]\n",
    );
    r.fails_with("evidence-state");
}

#[test]
fn a_test_that_tags_a_missing_item_is_refused() {
    covers(&[ids::check::evidence::TAGGED_MISSING]);
    let r = Repo::valid();
    r.edit("catalogue/cond/func.toml", "status = \"implemented\"\n", "");
    r.fails_with("evidence-tagged-missing");
}

fn with_evidence(r: &Repo, table: &str, tsv: &str) {
    let settings = r.read("catalogue/tally.toml");
    r.write(
        "catalogue/tally.toml",
        &format!("{settings}\n[[evidence]]\n{table}"),
    );
    r.write("data/implemented.tsv", tsv);
}

#[test]
fn an_evidence_file_that_agrees_with_the_catalogue_passes() {
    covers(&[ids::check::evidence::IMPORT, ids::refs::importers::TSV]);
    let r = Repo::valid();
    with_evidence(
        &r,
        "kind = \"tsv\"\npath = \"data/implemented.tsv\"\nstatus_column = \"state\"\nimplemented = [\"done\"]\ngroups = [\"cond.func\"]\n",
        "id\tstate\ncond.func.file\tdone\ncond.func.active\tdone\n",
    );
    r.passes(&[]);
}

#[test]
fn an_evidence_file_that_says_an_implemented_item_is_not_is_refused() {
    covers(&[ids::check::evidence::IMPORT]);
    let r = Repo::valid();
    with_evidence(
        &r,
        "kind = \"tsv\"\npath = \"data/implemented.tsv\"\nstatus_column = \"state\"\nimplemented = [\"done\"]\ngroups = [\"cond.func\"]\n",
        "id\tstate\ncond.func.file\tdone\ncond.func.active\topen\n",
    );
    r.fails_with("evidence-import");
}

#[test]
fn an_evidence_file_that_says_a_missing_item_is_implemented_is_refused() {
    covers(&[ids::check::evidence::IMPORT]);
    let r = Repo::valid();
    with_evidence(
        &r,
        "kind = \"tsv\"\npath = \"data/implemented.tsv\"\n",
        "id\ncond.func.file\ncond.func.active\nours.extra.speed\n",
    );
    r.fails_with("evidence-import");
}

#[test]
fn an_evidence_file_naming_an_unknown_item_is_refused() {
    covers(&[ids::check::evidence::IMPORT]);
    let r = Repo::valid();
    with_evidence(
        &r,
        "kind = \"tsv\"\npath = \"data/implemented.tsv\"\ngroups = [\"cond.func\"]\n",
        "id\ncond.func.file\ncond.func.active\ncond.func.many\n",
    );
    r.fails_with("evidence-import");
}

#[test]
fn an_evidence_file_without_its_key_column_is_refused() {
    covers(&[ids::check::evidence::IMPORT]);
    let r = Repo::valid();
    with_evidence(
        &r,
        "kind = \"tsv\"\npath = \"data/implemented.tsv\"\n",
        "name\ncond.func.file\n",
    );
    r.fails_with("evidence-file");
}

#[test]
fn an_evidence_file_without_its_status_column_is_refused() {
    covers(&[ids::check::evidence::IMPORT]);
    let r = Repo::valid();
    with_evidence(
        &r,
        "kind = \"tsv\"\npath = \"data/implemented.tsv\"\nstatus_column = \"state\"\n",
        "id\ncond.func.file\n",
    );
    r.fails_with("evidence-file");
}

#[test]
fn a_missing_evidence_file_is_refused() {
    covers(&[ids::check::evidence::IMPORT]);
    let r = Repo::valid();
    with_evidence(
        &r,
        "kind = \"tsv\"\npath = \"data/implemented.tsv\"\n",
        "id\n",
    );
    r.remove("data/implemented.tsv");
    r.fails_with("evidence-file");
}

/// A group of three engine functions; the ledger says the first and the third are done.
fn with_ledger(r: &Repo, statuses: [&str; 3], declared: [&str; 3]) {
    let addrs = ["00401000", "00401020", "00401030"];
    let mut items = String::new();
    let mut rows = String::new();
    for ((a, d), st) in addrs.iter().zip(declared).zip(statuses) {
        let _ = write!(
            items,
            "\n[[item]]\nid = \"f{a}\"\ntitle = \"0x{a}\"\nstatus = \"{d}\"\n"
        );
        let _ = writeln!(rows, "{a}\t16\tFn_{a}\tcore\tunit\t{st}\t\t");
    }
    r.write(
        "catalogue/engine/fns.toml",
        &format!("[group]\nid = \"engine.fns\"\ntitle = \"Engine functions\"\norigin = \"original\"\n{items}"),
    );
    with_evidence(
        r,
        "kind = \"ledger\"\npath = \"data/implemented.tsv\"\ngroups = [\"engine.fns\"]\n",
        &format!("address\tsize\tname\tsubsystem\tunit\tstatus\trust\ttyped\n{rows}"),
    );
}

#[test]
fn a_ledger_that_agrees_with_the_catalogue_passes() {
    covers(&[ids::check::evidence::IMPORT, ids::refs::importers::LEDGER]);
    let r = Repo::valid();
    with_ledger(
        &r,
        ["translated", "open", "static init (table)"],
        ["implemented", "missing", "implemented"],
    );
    r.passes(&[]);
}

#[test]
fn a_ledger_that_says_an_implemented_function_is_open_is_refused() {
    covers(&[ids::check::evidence::IMPORT, ids::refs::importers::LEDGER]);
    let r = Repo::valid();
    with_ledger(
        &r,
        ["translated", "open", "traced"],
        ["implemented", "missing", "implemented"],
    );
    let out = r.fails_with("evidence-import");
    assert!(out.contains("engine.fns.f00401030"), "{out}");
}

#[test]
fn a_ledger_that_says_a_missing_function_is_translated_is_refused() {
    covers(&[ids::check::evidence::IMPORT, ids::refs::importers::LEDGER]);
    let r = Repo::valid();
    with_ledger(
        &r,
        ["translated", "translated", "static init (table)"],
        ["implemented", "missing", "implemented"],
    );
    let out = r.fails_with("evidence-import");
    assert!(out.contains("engine.fns.f00401020"), "{out}");
}

#[test]
fn a_ledger_naming_a_function_the_catalogue_lacks_is_refused() {
    covers(&[ids::check::evidence::IMPORT, ids::refs::importers::LEDGER]);
    let r = Repo::valid();
    with_ledger(
        &r,
        ["translated", "open", "static init (table)"],
        ["implemented", "missing", "implemented"],
    );
    let text = r.read("data/implemented.tsv");
    r.write(
        "data/implemented.tsv",
        &format!("{text}00401040\t8\tExtra\tcore\tunit\topen\t\t\n"),
    );
    let out = r.fails_with("evidence-import");
    assert!(out.contains("engine.fns.f00401040"), "{out}");
}

#[test]
fn a_ledger_without_exactly_one_group_is_refused() {
    covers(&[ids::check::evidence::IMPORT, ids::refs::importers::LEDGER]);
    let r = Repo::valid();
    with_evidence(
        &r,
        "kind = \"ledger\"\npath = \"data/implemented.tsv\"\n",
        "address\tstatus\n00401000\ttranslated\n",
    );
    r.fails_with("evidence-file");
}

/// A group of three xNVSE commands, and a capabilities file that lists `listed` as implemented.
fn with_capabilities(r: &Repo, listed: &[&str], declared: [&str; 3]) {
    let mut items = String::new();
    for (c, d) in ["GetNVSEVersion", "GetWeight", "Return"]
        .iter()
        .zip(declared)
    {
        let id = tally::import::segment(c);
        let _ = write!(
            items,
            "\n[[item]]\nid = \"{id}\"\ntitle = \"`{c}`\"\nstatus = \"{d}\"\n"
        );
    }
    r.write(
        "catalogue/ext/xnvse.toml",
        &format!("[group]\nid = \"ext.xnvse\"\ntitle = \"xNVSE\"\norigin = \"original\"\n{items}"),
    );
    let names: Vec<String> = listed.iter().map(|n| format!("\"{n}\"")).collect();
    let json = format!(
        r#"{{"schema_version": 1, "extenders": [
  {{"id": "xnvse", "name": "xNVSE", "commands": {{"implemented": {}, "implemented_names": [{}], "total": 3}}}},
  {{"id": "jip-ln", "name": "JIP LN", "commands": {{"implemented": 1, "implemented_names": ["GetLNVersion"]}}}}
]}}"#,
        listed.len(),
        names.join(", ")
    );
    with_evidence(
        r,
        "kind = \"capabilities\"\npath = \"data/implemented.tsv\"\ngroups = [\"ext.xnvse\"]\n",
        &json,
    );
}

#[test]
fn a_capabilities_file_that_agrees_with_the_catalogue_passes() {
    covers(&[
        ids::check::evidence::IMPORT,
        ids::refs::importers::CAPABILITIES,
    ]);
    let r = Repo::valid();
    with_capabilities(
        &r,
        &["GetNVSEVersion", "Return"],
        ["implemented", "missing", "implemented"],
    );
    r.passes(&[]);
}

#[test]
fn a_capabilities_file_that_lacks_an_implemented_command_is_refused() {
    covers(&[
        ids::check::evidence::IMPORT,
        ids::refs::importers::CAPABILITIES,
    ]);
    let r = Repo::valid();
    with_capabilities(
        &r,
        &["GetNVSEVersion"],
        ["implemented", "missing", "implemented"],
    );
    let out = r.fails_with("evidence-import");
    assert!(out.contains("ext.xnvse.return_"), "{out}");
}

#[test]
fn a_capabilities_file_that_lists_a_missing_command_is_refused() {
    covers(&[
        ids::check::evidence::IMPORT,
        ids::refs::importers::CAPABILITIES,
    ]);
    let r = Repo::valid();
    with_capabilities(
        &r,
        &["GetNVSEVersion", "GetWeight", "Return"],
        ["implemented", "missing", "implemented"],
    );
    let out = r.fails_with("evidence-import");
    assert!(out.contains("ext.xnvse.getweight"), "{out}");
}

#[test]
fn a_capabilities_file_naming_an_unknown_command_is_refused() {
    covers(&[
        ids::check::evidence::IMPORT,
        ids::refs::importers::CAPABILITIES,
    ]);
    let r = Repo::valid();
    with_capabilities(
        &r,
        &["GetNVSEVersion", "Return", "Teleport"],
        ["implemented", "missing", "implemented"],
    );
    let out = r.fails_with("evidence-import");
    assert!(out.contains("ext.xnvse.teleport"), "{out}");
}

#[test]
fn a_capabilities_group_without_its_extender_is_refused() {
    covers(&[
        ids::check::evidence::IMPORT,
        ids::refs::importers::CAPABILITIES,
    ]);
    let r = Repo::valid();
    with_capabilities(
        &r,
        &["GetNVSEVersion", "Return"],
        ["implemented", "missing", "implemented"],
    );
    let text = r
        .read("data/implemented.tsv")
        .replace("\"xnvse\"", "\"kenvse\"");
    r.write("data/implemented.tsv", &text);
    let out = r.fails_with("evidence-file");
    assert!(out.contains("matches no extender"), "{out}");
}

#[test]
fn a_planned_evidence_importer_is_refused_with_its_issue() {
    covers(&[ids::check::evidence::IMPORT]);
    let r = Repo::valid();
    with_evidence(&r, "kind = \"contracts\"\npath = \"contracts.json\"\n", "");
    let out = r.fails_with("evidence-planned");
    assert!(out.contains("tally/issues/5"), "{out}");
}

#[test]
fn an_unknown_evidence_importer_is_refused() {
    covers(&[ids::check::evidence::IMPORT]);
    let r = Repo::valid();
    with_evidence(&r, "kind = \"yaml\"\npath = \"x.yaml\"\n", "");
    r.fails_with("evidence-kind");
}

// Tags.

#[test]
fn a_tag_naming_an_unknown_id_is_refused() {
    covers(&[ids::check::tags::UNKNOWN]);
    let r = Repo::valid();
    r.write(
        "tests/cases/one/covers.toml",
        "covers = [\"cond.func.many\"]\n",
    );
    r.fails_with("tag-unknown");
}

#[test]
fn a_tag_naming_a_retired_id_is_refused() {
    covers(&[ids::check::tags::UNKNOWN]);
    let r = Repo::valid();
    r.edit(
        "catalogue/cond/func.toml",
        "label = \"supported, except regex\"\n",
        "retired = \"merged into file\"\n",
    );
    r.edit(
        "catalogue/cond/func.toml",
        "ref = [\"up:file\"]",
        "ref = [\"up:file\", \"up:active\"]",
    );
    let out = r.fails_with("tag-unknown");
    assert!(out.contains("is retired"), "{out}");
}

#[test]
fn a_tag_naming_a_renamed_id_says_the_new_id() {
    covers(&[ids::check::tags::UNKNOWN]);
    let r = Repo::valid();
    r.edit(
        "catalogue/cond/func.toml",
        "id = \"active\"\n",
        "id = \"active_plugin\"\naliases = [\"cond.func.active\"]\n",
    );
    let out = r.fails_with("tag-unknown");
    assert!(
        out.contains("renamed to `cond.func.active_plugin`"),
        "{out}"
    );
}

#[test]
fn a_covers_call_with_a_short_path_is_refused() {
    covers(&[ids::check::tags::PATH]);
    let r = Repo::valid();
    r.edit(
        "src/lib.rs",
        "covers(&[ids::cond::func::FILE]);",
        "covers(&[func::FILE]);",
    );
    r.fails_with("tag-path");
}

#[test]
fn a_covers_call_with_an_expression_is_refused() {
    covers(&[ids::check::tags::PATH]);
    let r = Repo::valid();
    r.edit(
        "src/lib.rs",
        "covers(&[ids::cond::func::FILE]);",
        "covers(&[pick(1)]);",
    );
    r.fails_with("tag-path");
}

#[test]
fn an_empty_covers_call_is_refused() {
    covers(&[ids::check::tags::PATH]);
    let r = Repo::valid();
    r.edit(
        "src/lib.rs",
        "covers(&[ids::cond::func::FILE]);",
        "covers(&[]);",
    );
    r.fails_with("tag-path");
}

#[test]
fn a_covers_call_outside_a_test_is_refused() {
    covers(&[ids::check::tags::OUTSIDE_TEST]);
    let r = Repo::valid();
    r.edit(
        "src/lib.rs",
        "pub fn file() {}",
        "pub fn file() {\n    covers(&[ids::cond::func::FILE]);\n}",
    );
    r.fails_with("tag-outside-test");
}

fn eleven() -> String {
    ["ids::cond::func::FILE"; 11].join(", ")
}

#[test]
fn a_test_that_tags_more_than_ten_ids_is_refused() {
    covers(&[ids::check::tags::BULK]);
    let r = Repo::valid();
    r.edit(
        "src/lib.rs",
        "covers(&[ids::cond::func::FILE]);",
        &format!("covers(&[{}]);", eleven()),
    );
    r.fails_with("tag-bulk");
}

#[test]
fn a_table_test_with_a_covers_table_reason_may_tag_more_than_ten_ids() {
    covers(&[ids::check::tags::BULK]);
    let r = Repo::valid();
    r.edit(
        "src/lib.rs",
        "covers(&[ids::cond::func::FILE]);",
        &format!(
            "// covers-table: one row per function\n        covers(&[{}]);",
            eleven()
        ),
    );
    r.passes(&[]);
}

#[test]
fn a_data_case_that_tags_more_than_ten_ids_is_refused() {
    covers(&[ids::check::tags::BULK]);
    let r = Repo::valid();
    let list = ["\"cond.func.active\""; 11].join(", ");
    r.write(
        "tests/cases/one/covers.toml",
        &format!("covers = [{list}]\n"),
    );
    r.fails_with("tag-bulk");
    r.write(
        "tests/cases/one/covers.toml",
        &format!("covers = [{list}]\ncovers_table = \"every function once\"\n"),
    );
    r.passes(&[]);
}

#[test]
fn a_tag_in_an_ignored_test_is_warned_about_and_not_counted() {
    covers(&[ids::check::tags::IGNORED]);
    let r = Repo::valid();
    r.edit(
        "src/lib.rs",
        "#[test]\n",
        "#[test]\n    #[ignore = \"slow\"]\n",
    );
    let out = r.passes(&[]);
    assert!(out.contains("warning[tag-ignored]"), "{out}");
    let (_, report, _) = r.tally(&["report", "--depth", "3"]);
    assert!(report.contains("implemented cond.func.file"), "{report}");
}

#[test]
fn a_tag_on_a_not_applicable_item_is_warned_about() {
    covers(&[ids::check::tags::IGNORED]);
    let r = Repo::valid();
    r.write(
        "tests/cases/one/covers.toml",
        "covers = [\"cond.func.active\", \"ours.extra.other_game\"]\n",
    );
    let out = r.passes(&[]);
    assert!(out.contains("warning[tag-not-counted]"), "{out}");
}

#[test]
fn a_data_case_without_a_covers_list_is_refused() {
    covers(&[ids::check::tags::DATA]);
    let r = Repo::valid();
    r.write(
        "tests/cases/one/covers.toml",
        "tags = [\"cond.func.active\"]\n",
    );
    r.fails_with("tag-data");
}

#[test]
fn a_data_case_that_does_not_parse_is_refused() {
    covers(&[ids::check::tags::DATA]);
    let r = Repo::valid();
    r.write("tests/cases/one/covers.toml", "covers = [\n");
    r.fails_with("tag-data");
}

#[test]
fn a_data_case_with_a_number_in_its_list_is_refused() {
    covers(&[ids::check::tags::DATA]);
    let r = Repo::valid();
    r.write(
        "tests/cases/one/covers.toml",
        "covers = [\"cond.func.active\", 3]\n",
    );
    r.fails_with("tag-path");
}

#[test]
fn a_data_runner_that_is_not_a_test_is_refused() {
    covers(&[ids::check::tags::DATA]);
    let r = Repo::valid();
    r.write("tests/cases.rs", "fn every_case_passes() {}\n");
    r.fails_with("tag-data");
}

#[test]
fn a_data_pattern_that_matches_nothing_is_refused() {
    covers(&[ids::check::tags::DATA]);
    let r = Repo::valid();
    r.remove("tests/cases/one/covers.toml");
    r.fails_with("tag-data");
}

#[test]
fn a_data_runner_without_a_file_is_refused() {
    covers(&[ids::check::tags::DATA]);
    let r = Repo::valid();
    r.edit(
        "catalogue/tally.toml",
        "test = \"tests/cases.rs::every_case_passes\"",
        "test = \"every_case_passes\"",
    );
    r.fails_with("tag-data");
}

// Generator.

#[test]
fn a_multi_line_id_that_build_rs_cannot_read_is_refused() {
    covers(&[ids::check::generator::READER]);
    let r = Repo::valid();
    r.edit(
        "catalogue/ours/extra.toml",
        "id = \"speed\"",
        "id = \"\"\"speed\"\"\"",
    );
    r.fails_with("generator-reader");
}

#[test]
fn a_multi_line_alias_list_is_refused() {
    covers(&[ids::check::generator::READER]);
    let r = Repo::valid();
    r.edit(
        "catalogue/ours/extra.toml",
        "id = \"speed\"\n",
        "id = \"speed\"\naliases = [\n  \"ours.extra.fast\",\n]\n",
    );
    r.fails_with("generator-reader");
}

#[test]
fn an_id_in_a_literal_string_is_refused() {
    covers(&[ids::check::generator::READER]);
    let r = Repo::valid();
    r.edit(
        "catalogue/ours/extra.toml",
        "id = \"speed\"",
        "id = 'speed'",
    );
    r.fails_with("generator-reader");
}

#[test]
fn a_missing_generator_copy_is_refused() {
    covers(&[ids::check::generator::COPY]);
    let r = Repo::valid();
    r.remove("catalogue/tally_ids_gen.rs");
    r.fails_with("generator-copy");
}

#[test]
fn a_generator_copy_from_another_version_is_refused() {
    covers(&[ids::check::generator::COPY]);
    let r = Repo::valid();
    r.write(
        "catalogue/tally_ids_gen.rs",
        &format!("{}\n// edited\n", tally::check::GENERATOR),
    );
    r.fails_with("generator-copy");
}

#[test]
fn a_generator_copy_with_windows_line_ends_is_accepted() {
    covers(&[ids::check::generator::COPY]);
    let r = Repo::valid();
    r.write(
        "catalogue/tally_ids_gen.rs",
        &tally::check::GENERATOR.replace('\n', "\r\n"),
    );
    r.passes(&[]);
}

// History.

#[test]
fn an_id_removed_since_the_base_is_refused() {
    covers(&[ids::check::history::REMOVED]);
    let r = Repo::valid();
    let base = r.commit();
    r.edit(
        "catalogue/ours/extra.toml",
        "[[item]]\nid = \"speed\"\ntitle = \"Fast sort\"\n",
        "",
    );
    let out = r.fails_with_args("removed", &["--base", &base]);
    assert!(out.contains("ours.extra.speed"), "{out}");
}

#[test]
fn an_id_retired_since_the_base_passes() {
    covers(&[ids::check::history::REMOVED]);
    let r = Repo::valid();
    let base = r.commit();
    r.edit(
        "catalogue/ours/extra.toml",
        "title = \"Fast sort\"\n",
        "title = \"Fast sort\"\nretired = \"dropped\"\n",
    );
    r.passes(&["--base", &base]);
}

#[test]
fn an_id_renamed_with_an_alias_since_the_base_passes() {
    covers(&[ids::check::history::REMOVED]);
    let r = Repo::valid();
    let base = r.commit();
    r.edit(
        "catalogue/ours/extra.toml",
        "id = \"speed\"\n",
        "id = \"fast_sort\"\naliases = [\"ours.extra.speed\"]\n",
    );
    r.passes(&["--base", &base]);
}

#[test]
fn an_alias_removed_since_the_base_is_refused() {
    covers(&[ids::check::history::REMOVED]);
    let r = Repo::valid();
    r.edit(
        "catalogue/ours/extra.toml",
        "id = \"speed\"\n",
        "id = \"fast_sort\"\naliases = [\"ours.extra.speed\"]\n",
    );
    let base = r.commit();
    r.edit(
        "catalogue/ours/extra.toml",
        "aliases = [\"ours.extra.speed\"]\n",
        "",
    );
    r.fails_with_args("removed", &["--base", &base]);
}

#[test]
fn a_retired_id_used_again_is_refused() {
    covers(&[ids::check::history::UNRETIRED]);
    let r = Repo::valid();
    r.edit(
        "catalogue/ours/extra.toml",
        "title = \"Fast sort\"\n",
        "title = \"Fast sort\"\nretired = \"dropped\"\n",
    );
    let base = r.commit();
    r.edit("catalogue/ours/extra.toml", "retired = \"dropped\"\n", "");
    r.fails_with_args("unretired", &["--base", &base]);
}

#[test]
fn a_new_implemented_item_without_a_tag_is_warned_about() {
    covers(&[ids::check::history::UNTAGGED_NEW]);
    let r = Repo::valid();
    let base = r.commit();
    let text = r.read("catalogue/ours/extra.toml");
    r.write(
        "catalogue/ours/extra.toml",
        &format!("{text}\n[[item]]\nid = \"new_thing\"\nstatus = \"implemented\"\n"),
    );
    let out = r.passes(&["--base", &base]);
    assert!(out.contains("warning[untagged-new]"), "{out}");
}

#[test]
fn a_base_that_git_does_not_know_is_refused() {
    covers(&[ids::check::history::REMOVED]);
    let r = Repo::valid();
    r.commit();
    r.fails_with_args("base", &["--base", "no-such-revision"]);
}

#[test]
fn a_base_without_a_catalogue_passes() {
    covers(&[ids::check::history::REMOVED]);
    let r = Repo::valid();
    std::fs::rename(r.path("catalogue"), r.path("hidden")).unwrap();
    let base = r.commit();
    std::fs::rename(r.path("hidden"), r.path("catalogue")).unwrap();
    r.passes(&["--base", &base]);
}

#[test]
fn ids_from_an_items_from_list_at_the_base_must_stay() {
    covers(&[ids::check::history::REMOVED, ids::typed::ids::ITEMS_FROM]);
    let r = Repo::valid();
    r.write(
        "catalogue/ref/fns.tsv",
        "key\ttitle\nf_0001\tfirst\nf_0002\tsecond\n",
    );
    r.write("catalogue/fns/all.toml", "[group]\nid = \"fns.all\"\ntitle = \"Functions\"\norigin = \"original\"\nitems_from = \"ref/fns.tsv\"\n");
    let base = r.commit();
    r.passes(&["--base", &base]);
    r.write("catalogue/ref/fns.tsv", "key\ttitle\nf_0001\tfirst\n");
    r.fails_with_args("removed", &["--base", &base]);
}

fn report_json(r: &Repo, covered: usize, tagged: usize) -> String {
    let path = r.path("last-report.json");
    std::fs::write(
        &path,
        format!(
            "{{\"schema\":\"tally-report.v1\",\"totals\":{{\"total\":3,\"missing\":1,\"implemented\":2,\"tagged\":{tagged},\"passing\":0,\"covered\":{covered},\"gated\":0,\"not_applicable\":1,\"deferred\":0}}}}"
        ),
    )
    .unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn a_covered_count_below_the_last_report_is_refused() {
    covers(&[ids::check::history::RATCHET]);
    let r = Repo::valid();
    let last = report_json(&r, 2, 0);
    r.fails_with_args("ratchet", &["--ratchet", &last]);
}

#[test]
fn a_tagged_count_below_the_last_report_is_refused() {
    covers(&[ids::check::history::RATCHET]);
    let r = Repo::valid();
    let last = report_json(&r, 0, 5);
    r.fails_with_args("ratchet", &["--ratchet", &last]);
}

#[test]
fn a_drop_within_the_allowed_drop_passes() {
    covers(&[ids::check::history::RATCHET]);
    let r = Repo::valid();
    let last = report_json(&r, 2, 2);
    r.passes(&["--ratchet", &last, "--allow-drop", "2"]);
}

#[test]
fn the_same_counts_as_the_last_report_pass() {
    covers(&[ids::check::history::RATCHET]);
    let r = Repo::valid();
    let last = report_json(&r, 0, 2);
    r.passes(&["--ratchet", &last]);
}

#[test]
fn a_ratchet_file_of_another_kind_is_a_usage_error() {
    covers(&[ids::check::history::RATCHET, ids::cmd::cli::USAGE]);
    let r = Repo::valid();
    let path = r.path("x.json");
    std::fs::write(&path, "{\"schema\":\"other\"}").unwrap();
    let (code, _) = r.check(&["--ratchet", &path.to_string_lossy()]);
    assert_eq!(code, 2);
}

#[test]
fn the_sha_helper_is_the_pin_hash() {
    covers(&[ids::check::pins::SHA256]);
    assert_eq!(
        sha(""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn ids_split_over_several_calls_in_one_test_still_count_for_the_bulk_guard() {
    covers(&[ids::check::tags::BULK]);
    let r = Repo::valid();
    let six = ["ids::cond::func::FILE"; 6].join(", ");
    r.edit(
        "src/lib.rs",
        "covers(&[ids::cond::func::FILE]);",
        &format!("covers(&[{six}]);\n        covers(&[{six}]);"),
    );
    r.fails_with("tag-bulk");
}

#[test]
fn a_base_whose_group_file_does_not_parse_is_refused() {
    covers(&[ids::check::history::REMOVED]);
    let r = Repo::valid();
    r.edit(
        "catalogue/ours/extra.toml",
        "origin = \"ours\"",
        "origin = \"elsewhere\"",
    );
    let base = r.commit();
    r.edit(
        "catalogue/ours/extra.toml",
        "origin = \"elsewhere\"",
        "origin = \"ours\"",
    );
    r.fails_with_args("base", &["--base", &base]);
}

#[test]
fn an_item_retired_since_the_base_is_warned_about() {
    covers(&[ids::check::history::REMOVED]);
    let r = Repo::valid();
    let base = r.commit();
    r.edit(
        "catalogue/ours/extra.toml",
        "title = \"Fast sort\"\n",
        "title = \"Fast sort\"\nretired = \"dropped\"\n",
    );
    let out = r.passes(&["--base", &base]);
    assert!(out.contains("warning[retired-new]"), "{out}");
}

#[test]
fn a_path_that_leaves_the_repository_is_refused() {
    covers(&[ids::check::ids::PARSE]);
    let r = Repo::valid();
    r.edit(
        "catalogue/cond/func.toml",
        "src/lib.rs#pub fn file",
        "../outside/lib.rs",
    );
    r.fails_with("path-escape");
}

#[test]
fn an_empty_list_of_scan_paths_is_refused() {
    covers(&[ids::check::tags::DATA]);
    let r = Repo::valid();
    let settings = r.read("catalogue/tally.toml");
    r.write(
        "catalogue/tally.toml",
        &format!("[scan]\npaths = []\n\n{settings}"),
    );
    r.fails_with("settings");
}

#[test]
fn a_scan_path_that_does_not_exist_is_refused() {
    covers(&[ids::check::tags::DATA]);
    let r = Repo::valid();
    let settings = r.read("catalogue/tally.toml");
    r.write(
        "catalogue/tally.toml",
        &format!("[scan]\npaths = [\"src\", \"tsets\"]\n\n{settings}"),
    );
    r.fails_with("settings");
}

#[test]
fn turning_the_generator_check_off_is_warned_about() {
    covers(&[ids::check::generator::COPY]);
    let r = Repo::valid();
    let settings = r.read("catalogue/tally.toml");
    r.write(
        "catalogue/tally.toml",
        &format!("[ids]\ngenerator = \"\"\n\n{settings}"),
    );
    let out = r.passes(&[]);
    assert!(out.contains("warning[generator-off]"), "{out}");
}

#[test]
fn an_evidence_scope_naming_an_unknown_group_is_refused() {
    covers(&[ids::check::evidence::IMPORT]);
    let r = Repo::valid();
    with_evidence(
        &r,
        "kind = \"tsv\"\npath = \"data/implemented.tsv\"\ngroups = [\"no.such\"]\n",
        "id\n",
    );
    r.fails_with("evidence-import");
}
