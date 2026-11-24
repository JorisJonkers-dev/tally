// SPDX-License-Identifier: GPL-3.0-only
//! The ids module that build.rs writes: tally's own, from its own catalogue.

mod common;

use common::{covers, ids};
use tally::ids::tally_ids_gen as idg;

#[test]
fn a_constant_holds_its_dotted_id() {
    covers(&[ids::typed::ids::MODULE]);
    assert_eq!(ids::check::pins::SHA256.as_str(), "check.pins.sha256");
}

#[test]
fn covers_appends_one_line_per_id_when_the_log_is_set() {
    covers(&[ids::typed::ids::COVERS_LOG]);
    // The variable is set for a child run of this test binary (setting it here would need `unsafe`).
    let path = std::env::temp_dir().join(format!("tally-covers-log-{}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "covers_log_child", "--test-threads", "1"])
        .env("TALLY_COVERS_LOG", &path)
        .status()
        .unwrap();
    assert!(status.success());
    let text = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(text, "covers_log_child\ttyped.ids.covers_log\n");
}

/// Run by the test above with `TALLY_COVERS_LOG` set; on its own it only tags.
#[test]
fn covers_log_child() {
    covers(&[ids::typed::ids::COVERS_LOG]);
}

fn group(text: &str) -> idg::GenGroup {
    idg::read_group_text(std::path::Path::new("g.toml"), text).unwrap()
}

#[test]
fn an_alias_becomes_a_deprecated_constant_naming_the_new_id() {
    covers(&[ids::typed::ids::ALIAS]);
    let g = group(
        "[group]\nid = \"a.b\"\n\n[[item]]\nid = \"new_name\"\naliases = [\"a.b.old_name\"] # renamed\n",
    );
    let src = idg::module_source(&[g]).unwrap();
    assert!(src.contains("#[deprecated(note = \"renamed to a.b.new_name\")]\n            pub const OLD_NAME: super::super::super::Id = super::super::super::Id(\"a.b.new_name\");"), "{src}");
}

#[test]
fn a_retired_item_and_its_aliases_have_no_constant() {
    covers(&[ids::typed::ids::RETIRED]);
    let g = group(
        "[group]\nid = \"a.b\"\n\n[[item]]\nid = \"gone\"\nretired = \"merged\"\naliases = [\"a.b.older\"]\n\n[[item]]\nid = \"kept\"\n",
    );
    let src = idg::module_source(&[g]).unwrap();
    assert!(
        !src.contains("GONE") && !src.contains("OLDER") && src.contains("KEPT"),
        "{src}"
    );
}

#[test]
fn the_generator_refuses_keywords_and_duplicates() {
    covers(&[ids::typed::ids::MODULE, ids::check::ids::SYNTAX]);
    assert!(
        idg::module_source(&[group("[group]\nid = \"a.type\"\n\n[[item]]\nid = \"x\"\n")]).is_err()
    );
    assert!(
        idg::module_source(&[group(
            "[group]\nid = \"a.b\"\n\n[[item]]\nid = \"x\"\n\n[[item]]\nid = \"x\"\n"
        )])
        .is_err()
    );
    assert!(
        idg::valid_segment("f_0001")
            && !idg::valid_segment("1st")
            && !idg::valid_segment("")
            && !idg::valid_segment("self")
    );
}

#[test]
fn the_line_reader_refuses_a_group_without_an_id_or_an_item_without_one() {
    covers(&[ids::check::generator::READER]);
    assert!(
        idg::read_group_text(std::path::Path::new("g.toml"), "[group]\ntitle = \"x\"\n").is_err()
    );
    assert!(
        idg::read_group_text(
            std::path::Path::new("g.toml"),
            "[group]\nid = \"a.b\"\n[[item]]\ntitle = \"x\"\n"
        )
        .is_err()
    );
    assert!(
        idg::read_group_text(
            std::path::Path::new("g.toml"),
            "[group]\nid = \"a.b\" trailing\n"
        )
        .is_err()
    );
}

#[test]
fn tally_builds_its_own_ids_from_its_own_catalogue() {
    covers(&[ids::typed::ids::MODULE]);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let groups = idg::read_catalogue(&root.join("catalogue")).unwrap();
    let src = idg::module_source(&groups).unwrap();
    let built = std::fs::read_to_string(concat!(env!("OUT_DIR"), "/tally_ids.rs")).unwrap();
    assert_eq!(src, built);
}

#[test]
fn tally_passes_its_own_check() {
    covers(&[ids::cmd::cli::CHECK]);
    let root = env!("CARGO_MANIFEST_DIR");
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = tally::cli::run(
        &["check".into(), "--root".into(), root.into()],
        &mut out,
        &mut err,
    );
    let text = String::from_utf8_lossy(&out);
    assert_eq!(code, 0, "{text}{}", String::from_utf8_lossy(&err));
    assert!(text.contains("0 errors, 0 warnings"), "{text}");
}

fn compile(body: &str) -> (bool, String) {
    let dir = std::env::temp_dir().join(format!(
        "tally-compile-{}-{}",
        std::process::id(),
        body.len()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let module = concat!(env!("OUT_DIR"), "/tally_ids.rs").replace('\\', "/");
    let src = dir.join("main.rs");
    std::fs::write(&src, format!("mod t {{\n    include!(\"{module}\");\n}}\nuse t::{{covers, ids, Id}};\nfn main() {{\n    {body}\n}}\n")).unwrap();
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let out = std::process::Command::new(rustc)
        .args([
            "--edition",
            "2024",
            "--crate-type",
            "bin",
            "--emit",
            "metadata",
            "-o",
        ])
        .arg(dir.join("out.rmeta"))
        .arg(&src)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn a_test_naming_an_id_the_catalogue_has_compiles() {
    covers(&[ids::typed::ids::COMPILE_ERROR]);
    let (ok, err) = compile("let _ = Id::as_str; covers(&[ids::check::pins::SHA256]);");
    assert!(ok, "{err}");
}

#[test]
fn a_test_naming_an_id_the_catalogue_lacks_does_not_compile() {
    covers(&[ids::typed::ids::COMPILE_ERROR]);
    let (ok, err) = compile("covers(&[ids::check::pins::NO_SUCH_ITEM]);");
    assert!(!ok);
    assert!(err.contains("NO_SUCH_ITEM"), "{err}");
}

#[test]
fn a_test_cannot_make_an_id_of_its_own() {
    covers(&[ids::typed::ids::COMPILE_ERROR]);
    let (ok, err) = compile("covers(&[Id(\"check.pins.fake\")]);");
    assert!(!ok);
    assert!(err.contains("private"), "{err}");
}
