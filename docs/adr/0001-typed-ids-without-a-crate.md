# ADR 0001: typed ids from a copied generator, not from a crate or a tool run

Date: 2026-11-24. Status: accepted.

## Context

Tests name the catalogue items they prove. The owner chose typed ids (decision D2 of the coverage design): each
repository gets an ids module generated from its catalogue, tests call `covers(&[ids::area::group::ITEM])`, and a
wrong or removed id is a compile error. No proc macro, and no generated file is committed.

The module has to be generated at build time, by `build.rs`. The repositories this serves (loot-rs, nv-rs,
lodwright-rs, ttwright-rs, nvmod-rs, raul) have strict supply-chain rules: crates only from the registry, seven days
after publication, and a `deny.toml` that must not change. Their tests run on Linux, Windows and macOS.

## Options

1. **build.rs runs the pinned tally binary** (`tally ids --out $OUT_DIR/tally_ids.rs`). No code is copied, but every
   build, on every developer machine and every CI runner of every OS, first needs the right tally binary on the
   path. A missing binary fails `cargo build` of a library that has nothing to do with tests, and a different
   version gives a different module. It also makes the repositories depend on a non-cargo tool to compile at all.
2. **A shared crate on the registry** (`tally-ids` as a build-dependency). Normal Rust, but it is a new crate in
   every lock file, has to be published and then wait seven days for each change, and enters every `deny.toml`
   review. It would also need its own TOML parser (more crates) or a hand-written one.
3. **A tiny generator, copied** (chosen). `src/idsgen.rs` is one file that uses the standard library only. A
   repository copies it next to its catalogue (`tally ids --generator catalogue/tally_ids_gen.rs`) and its build.rs
   is three lines:

   ```rust
   include!("catalogue/tally_ids_gen.rs");
   fn main() {
       tally_ids_gen::build("catalogue");
   }
   ```

## Decision

Option 3. It needs no new crate, no network, no extra binary to build, and works the same on every OS.

What keeps the copy honest:

- `tally check` fails (`generator-copy`) when the copy differs from the generator of the tally version the
  repository pins, so an edited or stale copy cannot pass CI. Updating tally means running
  `tally ids --generator` once.
- The generator reads only the lines it needs (`id`, `retired`, `aliases`, `items_from`), so it needs no TOML
  parser. `tally check` reads every group file both ways and fails (`generator-reader`) when the line reader and
  the TOML reader disagree, for example on a multi-line string. Writing those keys on one line is the only rule.
- tally compiles the same file: its own build.rs includes `src/idsgen.rs`, and `tally ids --out` calls the same
  function, so the binary and every build.rs produce the same module for the same catalogue.

`tally ids --out <path>` stays available for a repository that prefers option 1.

## Consequences

- The generated module defines `Id`, `covers()` and `ids::...`. `Id` has a private field, so only the generated
  constants can make one; an alias is a `#[deprecated]` constant naming the new id (a warning, an error under
  `-D warnings`); a retired item has no constant.
- `covers()` does nothing unless `TALLY_COVERS_LOG` names a file; then it appends `<test thread>\t<id>` lines, which
  can confirm the static scan.
- tally finds the tags by scanning sources for `covers(&[...])` and needs the full `ids::` path in each call; the
  compiler has already proved that each path exists.
