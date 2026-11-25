# Using tally

The [README](../README.md) is the short version. This page has what tally does in detail, its commands, how to
add it to a repository, and how to work on it.

## What it does

- **Catalogue**: `catalogue/<area>/<group>.toml` per group, with items, states, references and evidence;
  reference lists of the original program in `catalogue/ref/*.tsv`, pinned by SHA-256 in `catalogue/sources.toml`
  with the upstream tag, commit and the files they were read from. Format: [docs/FORMAT.md](FORMAT.md).
- **Typed ids**: build.rs writes an ids module from the catalogue, with no new crate (it includes a copy of
  tally's standard-library-only generator; [ADR 0001](adr/0001-typed-ids-without-a-crate.md)). Tests call
  `covers(&[ids::area::group::ITEM])`, so a wrong or removed id does not compile, and rustfmt and the IDE see
  ordinary constants.
- **Results**: `tally ingest` reads cargo-nextest JUnit (`report-skipped = "all"`) or the text output of
  `cargo test` (`--libtest`); skipped, ignored and retried tests never count. In a workspace each test file belongs
  to the package of the nearest `Cargo.toml` above it. An item is **covered** when a tagged test passed on Linux, Windows and macOS.
- **Checks** (`tally check`, exit 1 on any error): ids are valid, unique and in the file their id names; the
  generator copy and its line reader agree with this tally; pins match; every reference row has an item and every
  reference names a row; implementation evidence agrees with the catalogue; tags name live ids, inside tests, with
  full paths; covers-map rows (`[[map]]`, tests in files the repository does not edit) name existing tests; no more than ten ids per test without a `covers-table:` reason; with `--base`, no id disappears
  without `retired` or an alias and a retired id stays retired; with `--ratchet`, the covered and tagged counts do
  not drop (unless `--allow-drop`). Warnings: tags in ignored tests, new implemented items without a tag.
- **Reports**: `--depth 1` areas, `2` groups, `3` items with their tests; `term`, `md`, `json`
  (`tally-report.v1`) or `svg` (one stacked bar per area and for the headline: covered, passing, tagged, implemented and missing, with the count of each); `--missing`, `--origin original|ours`;
  `[report]` in `tally.toml` hides the first row or leaves large areas out of it (docs/FORMAT.md).

## Commands

```text
tally check  [--base <git rev>] [--results <file>] [--ratchet <report.json>] [--allow-drop <n>]
tally report [--depth 1|2|3] [--format term|md|json|svg] [--missing] [--origin original|ours] [--results <file>] [--out <file>]
tally refs check
tally refs fetch [--source <id>] [--cache <dir>] [--update]
tally ingest (--junit <file> | --libtest <file>)... [--os linux|windows|macos] [--out <file>] [--append]
tally ids --out <path>          # the ids module, as build.rs writes it
tally ids --generator <path>    # the generator copy build.rs includes
tally doc <file> [--check]      # fill the <!-- tally:group <id> --> tables of a hand-written document
```

Common options: `--root <dir>` (default `.`), `--catalogue <dir>` (default `<root>/catalogue`). Exit codes: 0 fine,
1 a check failed, 2 the command could not run.

## Using it in a repository

1. Write `catalogue/` (see [docs/FORMAT.md](FORMAT.md)) and run `tally ids --generator catalogue/tally_ids_gen.rs`.
2. Add `build.rs`:

   ```rust
   include!("catalogue/tally_ids_gen.rs");
   fn main() {
       tally_ids_gen::build("catalogue");
   }
   ```

3. Include the module where tests can reach it, for example `tests/common/mod.rs` or a `#[cfg(test)]` module of
   the library: `include!(concat!(env!("OUT_DIR"), "/tally_ids.rs"));`, and tag tests with `covers(&[...])`.
4. In CI, download the pinned release binary, check its SHA-256 and run `tally check --base <base sha>`:

   ```yaml
   - name: Catalogue (tally)
     env:
       TALLY_VERSION: v0.1.0
       TALLY_SHA256: <SHA-256 of tally-x86_64-unknown-linux-gnu from that release's SHA256SUMS>
       BASE: ${{ github.event.pull_request.base.sha || github.event.before }}
     run: |
       curl -fsSL -o "$RUNNER_TEMP/tally" "https://github.com/JorisJonkers-dev/tally/releases/download/$TALLY_VERSION/tally-x86_64-unknown-linux-gnu"
       echo "$TALLY_SHA256  $RUNNER_TEMP/tally" | sha256sum -c -
       chmod +x "$RUNNER_TEMP/tally"
       "$RUNNER_TEMP/tally" check --base "$BASE"
   ```

Reports, the JSON and the SVG are generated on `main` (by the progress bot), never committed in a pull request.
The pattern is loot-rs's `.github/workflows/tally-progress.yml`: a render job without a token, then a commit job
that copies a fixed list of report files and pushes them as the App. Make sure `.gitignore` does not match those
files (a general `coverage/` rule does), and check it in the pull request pipeline, so the commit on `main` cannot
fail on it:

```sh
git check-ignore --no-index -v docs/COVERAGE.md docs/coverage/tally-report.json docs/coverage/tally.svg && exit 1
```

## Development

`task check` runs what CI runs: format, clippy with warnings as errors, the tests with a 90 percent line coverage
gate, tally's check of its own catalogue (`catalogue/`, where every test tags what it proves), the release build,
cargo-deny, actionlint and zizmor, gitleaks. Releases come from release-please: merging its pull request tags
`vX.Y.Z` and attaches `tally-x86_64-unknown-linux-gnu`, `tally-x86_64-pc-windows-msvc.exe`,
`tally-universal-apple-darwin` and `SHA256SUMS`, with a build provenance attestation.
