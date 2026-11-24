// SPDX-License-Identifier: GPL-3.0-only
//! A throwaway repository with a valid catalogue, written by each test, that a test then breaks in one way.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

include!(concat!(env!("OUT_DIR"), "/tally_ids.rs"));

static COUNTER: AtomicUsize = AtomicUsize::new(0);

pub const LIST: &str = "key\ttitle\nfile\tfile(path)\nactive\tactive(path)\n";
pub const UPSTREAM: &str = "# Conditions\nfile(path), active(path)\n";

pub const FUNC: &str = r#"[group]
id = "cond.func"
title = "Condition functions"
origin = "original"
reference = "up"

[[item]]
id = "file"
title = "`file(path)`"
status = "implemented"
ref = ["up:file"]
evidence = ["src/lib.rs#pub fn file"]

[[item]]
id = "active"
title = "`active(path)`"
status = "partial"
label = "supported, except regex"
ref = ["up:active"]
"#;

pub const EXTRA: &str = r#"[group]
id = "ours.extra"
title = "Our additions"
origin = "ours"

[[item]]
id = "speed"
title = "Fast sort"

[[item]]
id = "other_game"
title = "Other games"
status = "n/a"
reason = "New Vegas only"
"#;

pub const LIB: &str = r"pub fn file() {}

#[cfg(test)]
mod tests {
    #[test]
    fn file_finds_a_file() {
        covers(&[ids::cond::func::FILE]);
    }
}
";

pub const CASES: &str = "#[test]\nfn every_case_passes() {}\n";

pub const SETTINGS: &str = r#"[repo]
package = "demo"

[areas]
cond = "Conditions"

[[data]]
files = "tests/cases/*/covers.toml"
test = "tests/cases.rs::every_case_passes"
"#;

pub struct Repo {
    pub root: PathBuf,
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub fn sha(text: &str) -> String {
    tally::sha256::hex(text.as_bytes())
}

pub fn sources(list: &str) -> String {
    format!(
        r#"[[source]]
id = "up"
name = "Upstream"
repo = "https://github.com/example/upstream"
tag = "1.2.3"
commit = "0123456789abcdef0123456789abcdef01234567"
licence = "MIT"
read = "2026-11-24"
extractor = "manual"
list = "ref/up.tsv"
sha256 = "{}"

[source.upstream]
"docs/conditions.md" = "{}"
"#,
        sha(list),
        sha(UPSTREAM)
    )
}

impl Repo {
    /// A repository whose catalogue passes every check.
    pub fn valid() -> Repo {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("tally-test-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let r = Repo { root };
        r.write(
            "Cargo.toml",
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        );
        r.write("catalogue/tally.toml", SETTINGS);
        r.write("catalogue/sources.toml", &sources(LIST));
        r.write("catalogue/ref/up.tsv", LIST);
        r.write("catalogue/cond/func.toml", FUNC);
        r.write("catalogue/ours/extra.toml", EXTRA);
        r.write("catalogue/tally_ids_gen.rs", tally::check::GENERATOR);
        r.write("src/lib.rs", LIB);
        r.write("tests/cases.rs", CASES);
        r.write(
            "tests/cases/one/covers.toml",
            "covers = [\"cond.func.active\"]\n",
        );
        r
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    pub fn write(&self, rel: &str, text: &str) {
        let p = self.path(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    pub fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.path(rel)).unwrap()
    }

    pub fn remove(&self, rel: &str) {
        fs::remove_file(self.path(rel)).unwrap();
    }

    /// Replaces `from` by `to` in a file; the text must be there.
    pub fn edit(&self, rel: &str, from: &str, to: &str) {
        let text = self.read(rel);
        assert!(text.contains(from), "{rel} does not contain {from:?}");
        self.write(rel, &text.replacen(from, to, 1));
    }

    /// Runs tally in this repository; returns (exit code, stdout, stderr).
    pub fn tally(&self, args: &[&str]) -> (i32, String, String) {
        let mut all: Vec<String> = args.iter().map(ToString::to_string).collect();
        all.push("--root".into());
        all.push(self.root.to_string_lossy().into_owned());
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = tally::cli::run(&all, &mut out, &mut err);
        (
            code,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    /// `tally check` with extra arguments; returns (exit code, output).
    pub fn check(&self, args: &[&str]) -> (i32, String) {
        let mut all = vec!["check"];
        all.extend_from_slice(args);
        let (code, out, err) = self.tally(&all);
        (code, out + &err)
    }

    /// Asserts that `tally check` fails with an error of `code`.
    pub fn fails_with(&self, code: &str) -> String {
        self.fails_with_args(code, &[])
    }

    pub fn fails_with_args(&self, code: &str, args: &[&str]) -> String {
        let (exit, out) = self.check(args);
        assert_eq!(
            exit, 1,
            "expected a failure with error[{code}], got:\n{out}"
        );
        assert!(
            out.contains(&format!("error[{code}]")),
            "expected error[{code}], got:\n{out}"
        );
        out
    }

    /// Asserts that `tally check` passes and returns its output.
    pub fn passes(&self, args: &[&str]) -> String {
        let (exit, out) = self.check(args);
        assert_eq!(exit, 0, "expected the check to pass, got:\n{out}");
        assert!(!out.contains("error["), "{out}");
        out
    }

    /// Makes the work tree a git repository with one commit (plumbing only, so no hooks run).
    pub fn commit(&self) -> String {
        let git = |args: &[&str]| {
            let o = Command::new("git")
                .arg("-C")
                .arg(&self.root)
                .args(args)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
                .output()
                .unwrap();
            assert!(
                o.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&o.stderr)
            );
            String::from_utf8(o.stdout).unwrap().trim().to_string()
        };
        if !self.path(".git").exists() {
            git(&["init", "-q"]);
        }
        git(&["add", "-A"]);
        let tree = git(&["write-tree"]);
        git(&["commit-tree", &tree, "-m", "base"])
    }
}

pub fn exists(p: &Path) -> bool {
    p.exists()
}
