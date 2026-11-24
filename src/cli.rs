// SPDX-License-Identifier: GPL-3.0-only
//! The command line. Exit codes: 0 success, 1 a check failed, 2 the command could not run (usage, files).

use crate::catalogue::{self, Catalogue, Origin};
use crate::check::{self, Level};
use crate::ids::tally_ids_gen as idg;
use crate::report::{self, Format};
use crate::results::{self, Results, Run};
use crate::state::Counts;
use crate::{doc, refs, scan};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const USAGE: &str = "\
tally: feature catalogue, test tags and coverage reports

usage:
  tally check  [--base <git rev>] [--results <file>] [--ratchet <report.json>] [--allow-drop <n>]
  tally report [--depth 1|2|3] [--format term|md|json|svg] [--missing] [--origin original|ours]
               [--results <file>] [--out <file>]
  tally refs check
  tally refs fetch [--source <id>] [--cache <dir>] [--update]
  tally ingest (--junit <file> | --libtest <file>)... [--os linux|windows|macos] [--out <file>] [--append]
  tally ids --out <path>          write the ids module (what build.rs writes)
  tally ids --generator <path>    write the generator copy build.rs includes
  tally doc <file> [--check]      fill the <!-- tally:group --> tables of a document
  tally --version

common options: --root <dir> (the repository, default .), --catalogue <dir> (default <root>/catalogue)
";

struct Args {
    rest: Vec<String>,
}

impl Args {
    fn flag(&mut self, name: &str) -> bool {
        if let Some(p) = self.rest.iter().position(|a| a == name) {
            self.rest.remove(p);
            true
        } else {
            false
        }
    }

    fn value(&mut self, name: &str) -> Result<Option<String>, String> {
        let Some(p) = self.rest.iter().position(|a| a == name) else {
            return Ok(None);
        };
        if p + 1 >= self.rest.len() {
            return Err(format!("{name} needs a value"));
        }
        let v = self.rest.remove(p + 1);
        self.rest.remove(p);
        Ok(Some(v))
    }

    fn values(&mut self, name: &str) -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        while let Some(v) = self.value(name)? {
            out.push(v);
        }
        Ok(out)
    }

    fn done(&self) -> Result<(), String> {
        match self.rest.first() {
            Some(a) => Err(format!("unexpected argument `{a}`")),
            None => Ok(()),
        }
    }
}

struct Ctx {
    root: PathBuf,
    dir: PathBuf,
}

fn ctx(a: &mut Args) -> Result<Ctx, String> {
    let root = PathBuf::from(a.value("--root")?.unwrap_or_else(|| ".".into()));
    let dir = a
        .value("--catalogue")?
        .map_or_else(|| root.join("catalogue"), |d| root.join(d));
    Ok(Ctx { root, dir })
}

fn load(c: &Ctx) -> Result<Catalogue, String> {
    catalogue::load(&c.root, &c.dir)
}

fn load_results(path: Option<String>) -> Result<Option<Results>, String> {
    path.map(|p| results::load(Path::new(&p))).transpose()
}

fn write_out(path: Option<&str>, text: &str, out: &mut dyn Write) -> Result<(), String> {
    match path {
        Some(p) => fs::write(p, text).map_err(|e| format!("{p}: {e}")),
        None => out.write_all(text.as_bytes()).map_err(|e| e.to_string()),
    }
}

fn cmd_check(mut a: Args, out: &mut dyn Write) -> Result<i32, String> {
    let c = ctx(&mut a)?;
    let base = a.value("--base")?;
    let results = load_results(a.value("--results")?)?;
    let ratchet = match a.value("--ratchet")? {
        None => None,
        Some(p) => {
            let text = fs::read_to_string(&p).map_err(|e| format!("{p}: {e}"))?;
            let v: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| format!("{p}: {e}"))?;
            if v.get("schema").and_then(serde_json::Value::as_str) != Some(report::SCHEMA) {
                return Err(format!("{p}: not a {} document", report::SCHEMA));
            }
            let totals = v.get("totals").cloned().ok_or(format!("{p}: no totals"))?;
            Some(serde_json::from_value::<Counts>(totals).map_err(|e| format!("{p}: {e}"))?)
        }
    };
    let allow_drop = match a.value("--allow-drop")? {
        None => 0,
        Some(n) => n
            .parse()
            .map_err(|_| format!("--allow-drop takes a whole number, not `{n}`"))?,
    };
    a.done()?;
    let cat = load(&c)?;
    let sc = scan::scan(&cat);
    let findings = check::run(
        &cat,
        &sc,
        &check::Options {
            base,
            results,
            ratchet,
            allow_drop,
        },
    );
    let errors = findings.iter().filter(|f| f.level == Level::Error).count();
    let warnings = findings.len() - errors;
    for f in &findings {
        let level = if f.level == Level::Error {
            "error"
        } else {
            "warning"
        };
        let _ = writeln!(out, "{level}[{}]: {}", f.code, f.message);
    }
    let items = cat.items().count();
    let tags: usize = sc.sites.iter().map(|s| s.ids.len()).sum();
    let _ = writeln!(
        out,
        "tally check: {errors} errors, {warnings} warnings ({} groups, {items} items, {} sources, {tags} tags in {} places)",
        cat.groups.len(),
        cat.sources.source.len(),
        sc.sites.len()
    );
    Ok(i32::from(errors > 0))
}

fn cmd_report(mut a: Args, out: &mut dyn Write) -> Result<i32, String> {
    let c = ctx(&mut a)?;
    let depth = match a.value("--depth")?.as_deref() {
        None | Some("2") => 2,
        Some("1") => 1,
        Some("3") => 3,
        Some(d) => return Err(format!("--depth is 1, 2 or 3, not `{d}`")),
    };
    let format = match a.value("--format")?.as_deref() {
        None | Some("term") => Format::Term,
        Some("md") => Format::Md,
        Some("json") => Format::Json,
        Some("svg") => Format::Svg,
        Some(f) => return Err(format!("--format is term, md, json or svg, not `{f}`")),
    };
    let missing = a.flag("--missing");
    let origin = match a.value("--origin")?.as_deref() {
        None => None,
        Some("original") => Some(Origin::Original),
        Some("ours") => Some(Origin::Ours),
        Some(o) => return Err(format!("--origin is original or ours, not `{o}`")),
    };
    let results = load_results(a.value("--results")?)?;
    let path = a.value("--out")?;
    a.done()?;
    let cat = load(&c)?;
    let sc = scan::scan(&cat);
    let o = report::Options {
        depth,
        format,
        missing,
        origin,
    };
    // The picture lists the missing items of each area, so it always reads the items.
    let build = report::Options {
        depth: if format == Format::Svg { 3 } else { depth },
        ..o.clone()
    };
    let r = report::build(&cat, &sc.sites, results.as_ref(), &build);
    write_out(path.as_deref(), &report::render(&r, &o), out)?;
    Ok(0)
}

fn cmd_refs(mut a: Args, out: &mut dyn Write) -> Result<i32, String> {
    let sub = if a.rest.is_empty() {
        String::new()
    } else {
        a.rest.remove(0)
    };
    let c = ctx(&mut a)?;
    match sub.as_str() {
        "check" => {
            a.done()?;
            let cat = load(&c)?;
            let (findings, lists) = refs::check(&cat);
            for f in &findings {
                let _ = writeln!(out, "error[{}]: {}", f.code, f.message);
            }
            let rows: usize = lists.values().map(Vec::len).sum();
            let _ = writeln!(
                out,
                "tally refs check: {} errors ({} sources, {rows} reference rows)",
                findings.len(),
                cat.sources.source.len()
            );
            Ok(i32::from(!findings.is_empty()))
        }
        "fetch" => {
            let only = a.value("--source")?;
            let cache = a
                .value("--cache")?
                .map_or_else(|| c.root.join("target/tally-refs"), PathBuf::from);
            let update = a.flag("--update");
            a.done()?;
            let cat = load(&c)?;
            let (lines, ok) = refs::fetch(&cat, only.as_deref(), &cache, update);
            for l in lines {
                let _ = writeln!(out, "{l}");
            }
            Ok(i32::from(!ok))
        }
        other => Err(format!("refs takes `check` or `fetch`, not `{other}`")),
    }
}

fn cmd_ingest(mut a: Args, out: &mut dyn Write) -> Result<i32, String> {
    // The common options are accepted; ingest reads and writes only the files it is given.
    let _ = ctx(&mut a)?;
    let junit = a.values("--junit")?;
    let libtest = a.values("--libtest")?;
    let os = a.value("--os")?;
    let path = a
        .value("--out")?
        .unwrap_or_else(|| "tally-results.json".into());
    let append = a.flag("--append");
    a.done()?;
    if junit.is_empty() && libtest.is_empty() {
        return Err("ingest needs at least one --junit <file> or --libtest <file>".into());
    }
    if let Some(o) = &os
        && !results::PLATFORMS.contains(&o.as_str())
    {
        return Err(format!("--os is linux, windows or macos, not `{o}`"));
    }
    let mut res = if append && Path::new(&path).exists() {
        results::load(Path::new(&path))?
    } else {
        Results::default()
    };
    let files = junit
        .into_iter()
        .map(|f| (f, false))
        .chain(libtest.into_iter().map(|f| (f, true)));
    for (f, is_libtest) in files {
        let os = os
            .clone()
            .or_else(|| results::os_from_name(&f).map(str::to_string))
            .ok_or(format!("{f}: say which OS it is from with --os"))?;
        let bytes = fs::read(&f).map_err(|e| format!("{f}: {e}"))?;
        let text = String::from_utf8_lossy(&bytes);
        let cases = if is_libtest {
            results::parse_libtest(&text)
        } else {
            results::parse_junit(&text)
        }
        .map_err(|e| format!("{f}: {e}"))?;
        let _ = writeln!(out, "{f}: {} cases ({os})", cases.len());
        res.runs.push(Run { os, cases });
    }
    let text = serde_json::to_string_pretty(&res).map_err(|e| e.to_string())? + "\n";
    fs::write(&path, text).map_err(|e| format!("{path}: {e}"))?;
    Ok(0)
}

fn cmd_ids(mut a: Args, out: &mut dyn Write) -> Result<i32, String> {
    let c = ctx(&mut a)?;
    let target = a.value("--out")?;
    let generator = a.value("--generator")?;
    a.done()?;
    match (target, generator) {
        (Some(p), None) => {
            let groups = idg::read_catalogue(&c.dir)?;
            let src = idg::module_source(&groups)?;
            fs::write(&p, src).map_err(|e| format!("{p}: {e}"))?;
            let _ = writeln!(out, "wrote {p}");
        }
        (None, Some(p)) => {
            fs::write(&p, check::GENERATOR).map_err(|e| format!("{p}: {e}"))?;
            let _ = writeln!(
                out,
                "wrote {p} (the generator of tally {})",
                env!("CARGO_PKG_VERSION")
            );
        }
        _ => return Err("ids takes one of --out <path> or --generator <path>".into()),
    }
    Ok(0)
}

fn cmd_doc(mut a: Args, out: &mut dyn Write) -> Result<i32, String> {
    let c = ctx(&mut a)?;
    let only_check = a.flag("--check");
    let file = if a.rest.is_empty() {
        return Err("doc needs a file".into());
    } else {
        a.rest.remove(0)
    };
    a.done()?;
    let cat = load(&c)?;
    let text = fs::read_to_string(&file).map_err(|e| format!("{file}: {e}"))?;
    let filled = doc::fill(&cat, &text).map_err(|e| format!("{file}: {e}"))?;
    if filled == text {
        let _ = writeln!(out, "{file} is up to date");
        return Ok(0);
    }
    if only_check {
        let _ = writeln!(
            out,
            "{file}: the generated tables differ from the catalogue; run `tally doc {file}`"
        );
        return Ok(1);
    }
    fs::write(&file, filled).map_err(|e| format!("{file}: {e}"))?;
    let _ = writeln!(out, "wrote {file}");
    Ok(0)
}

/// Runs tally with `args` (without the program name).
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let mut rest = args.to_vec();
    let cmd = if rest.is_empty() {
        String::new()
    } else {
        rest.remove(0)
    };
    let a = Args { rest };
    let result = match cmd.as_str() {
        "check" => cmd_check(a, out),
        "report" => cmd_report(a, out),
        "refs" => cmd_refs(a, out),
        "ingest" => cmd_ingest(a, out),
        "ids" => cmd_ids(a, out),
        "doc" => cmd_doc(a, out),
        "--version" | "-V" => {
            let _ = writeln!(out, "tally {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        "help" | "--help" | "-h" => {
            let _ = out.write_all(USAGE.as_bytes());
            Ok(0)
        }
        "" => Err("no command given".into()),
        other => Err(format!("unknown command `{other}`")),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            let _ = writeln!(err, "tally: {e}\n\n{USAGE}");
            2
        }
    }
}
