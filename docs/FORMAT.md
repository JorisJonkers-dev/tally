# Catalogue format

A repository keeps its catalogue in `catalogue/` (another directory with `--catalogue`; nv-rs uses
`.fork/catalogue`). Paths in the files are relative to the repository root unless said otherwise.

```text
catalogue/
  tally.toml              settings (optional)
  sources.toml            pinned reference lists (optional)
  ref/<name>.tsv          the reference lists
  tally_ids_gen.rs        the generator copy build.rs includes (tally ids --generator)
  <area>/<group>.toml     one file per group; deeper groups: <area>/<sub>/<group>.toml
```

## Ids

Every id is dotted lower-case segments: a letter, then letters, digits or `_`, and no Rust keyword. A group id has
at least two segments and names its file: group `metadata.plugin_entry` is `catalogue/metadata/plugin_entry.toml`.
An item id is the group id plus one segment (`metadata.plugin_entry.name_exact`). The first segment is the area.

Ids are never reused. An item that goes away keeps its entry with `retired = "<why>"`; an item that is renamed lists
its old full ids in `aliases`. `tally check --base <rev>` fails when an id of that revision is gone without either.

## Group file

```toml
[group]
id = "conditions.functions"
title = "Condition functions"
origin = "original"              # original (the program this repository reimplements) or ours
reference = "libloot"            # optional: a source of sources.toml
items_from = "ref/fns.tsv"       # optional: every row of this list is an item (key = local id)
items_from_status = "missing"    # optional: the status of those rows that have no [[item]] of their own
notes = "..."                    # optional
item_column = "Item"             # optional headings for `tally doc` tables (Feature, Status, Notes)
status_column = "Status"
notes_column = "Notes"

[[item]]
id = "file"
title = "`file(path)`"
status = "implemented"           # missing (default), partial, implemented, n/a, deferred
label = "supported, except ..."  # optional status text for `tally doc` tables
notes = "..."
reason = "..."                   # required for n/a and deferred
gated = "needs the game data"    # optional: implemented, but CI can never cover it
ref = ["libloot:conditions.file"]  # rows of reference lists this item answers
evidence = ["src/condition.rs#fn eval_file"]  # files (and text in them) that implement it
aliases = ["conditions.functions.old_name"]
retired = "why"                  # optional
origin = "ours"                  # optional: overrides the group's
```

Write `id`, `retired`, `aliases` and `items_from` on one line each, as plain double-quoted strings: build.rs reads
those lines without a TOML parser, and `tally check` fails when the two readers disagree.

## States

| State | Meaning |
| --- | --- |
| missing | declared missing |
| implemented | declared implemented or partial, no test tags it |
| tagged | implemented and at least one test that is not ignored tags it |
| passing | a tagging test passed on at least one OS and none failed there |
| covered | a tagging test passed, and none failed, on Linux, Windows and macOS |
| gated | implemented, but needs something CI does not have; never covered, shown on its own |
| n/a, deferred | out of the count, listed with their reason |
| retired | not shown; the id stays reserved |

A skipped or ignored test never counts, and neither does a test that needed a retry (`flakyFailure`). Results come
from cargo-nextest JUnit (`[profile.ci.junit]`, `report-skipped = "all"`) through `tally ingest`.

## Tags in tests

```rust
// tests/common/mod.rs, or a #[cfg(test)] module of the library
include!(concat!(env!("OUT_DIR"), "/tally_ids.rs"));

#[test]
fn a_missing_requirement_is_a_violation() {
    covers(&[ids::metadata::plugin_entry::REQ]);
    // ...
}
```

Name each id by its full `ids::...` path (the scanner reads the call; the compiler checks the path). The call must
be inside a `#[test]` function. More than ten ids in one test needs a `// covers-table: <reason>` comment in or
just above the test.

Data-driven cases (golden directories) list their ids in a small TOML file per case, run by one test:

```toml
# catalogue/tally.toml
[[data]]
files = "tests/golden/*/covers.toml"
test = "tests/golden.rs::every_case_prints_what_its_golden_file_says"
```

```toml
# tests/golden/req_missing/covers.toml
covers = ["metadata.plugin_entry.req"]
covers_table = "..."   # needed above ten ids
```

## tally.toml

```toml
[repo]
package = "loot-rs"      # the nextest binary id; read from Cargo.toml when absent
name = "loot-rs"         # display name

[scan]
paths = ["src", "tests"] # default

[ids]
generator = "catalogue/tally_ids_gen.rs"   # default; "" turns the copy check off

[areas]
metadata = "Metadata file"   # titles for reports

[[evidence]]             # an importer whose rows must agree with the declared states
kind = "tsv"             # tsv, ledger, capabilities or contracts
path = "data/implemented.tsv"
key_column = "id"
status_column = "state"
implemented = ["done"]
groups = ["engine.functions"]
```

Each importer turns its file into rows of an item id and whether it is implemented; `tally check` fails when a row
names an id the catalogue does not have (within `groups`) and when an item in `groups` is declared implemented or
partial but the file does not say so, or the other way round.

- `tsv`: `key_column` (default `id`) holds the full item id. With `status_column`, a row is implemented when its
  value is one of `implemented`; without, every row is.
- `ledger`: the per-function ledger of nv-rs (`scripts/ledger --tsv <file>`: `address`, `name`, `status`, ...).
  `groups` names exactly one group, the group of the functions; the item of the function at `00401000` is
  `<group>.f00401000` (`f` and eight lower-case hex digits). A function is implemented when its `status` is
  `translated` or `static init (table)` (override with `status_column` and `implemented`; the address column with
  `key_column`).

- `capabilities`: nv-rs `.fork/capabilities.json`, the script commands each built-in extender implements
  (`extenders[].id`, `commands.implemented_names`). `groups` lists one group per extender to compare: the group whose
  last segment is the extender id as a segment (`jip-ln` is `jip_ln`). Every listed command is an implemented item
  of that group; extenders without a group are left out, and a group without an extender is an error. A command's
  item is its name as a segment: lower case, other characters `_`, `x` in front when it does not start with a letter, `_` after a Rust
  keyword (`GetLNVersion` is `getlnversion`, `Return` is `return_`). A name listed twice counts once; two different
  names that become the same item are an error, and so is an `implemented` count that differs from the list.
  The reference lists (every command of an extender) come from the extenders' own repositories through `manual` or
  `tsv` sources.

- `contracts`: the JSON `ttwright contracts --json` prints, saved to a file by CI. `groups` names exactly one group;
  every manifest feature a Contract lists under `supports.features[].id` is an implemented item of it, the id as a
  segment (`asset_op:3` is `asset_op_3`, `variable:%DESTINATION%` is `variable__destination_`). A document that is
  not `ttwright-contracts`, a feature without an id and two features that become one item are errors.

```toml
[[evidence]]
kind = "capabilities"
path = ".fork/capabilities.json"
groups = ["extenders.xnvse", "extenders.jip_ln"]
```

```toml
[[evidence]]
kind = "contracts"
path = "target/contracts.json"   # ttwright contracts --json > target/contracts.json, before tally check
groups = ["install.features"]
```

```toml
[[evidence]]
kind = "ledger"
path = "target/ledger.tsv"   # written by CI before tally check
groups = ["engine.functions"]
```

## sources.toml

```toml
[[source]]
id = "libloot"
name = "libloot"
repo = "https://github.com/loot/libloot"
tag = "0.29.7"
commit = "<40 hex digits the tag points to>"
licence = "GPL-3.0-or-later"
read = "2026-11-24"              # the date the upstream files were read
extractor = "manual"             # manual (written by hand from the files below), tsv or ledger
list = "ref/libloot.tsv"         # relative to the catalogue directory
sha256 = "<SHA-256 of the list>"

[source.upstream]                # SHA-256 of each upstream file at that commit
"docs/metadata/conditions.rst" = "<hex>"

[source.tsv]                     # extractor = "tsv" only
from = "research/engine_map.tsv" # one of the upstream files
key_column = "addr"
title_column = "name"
where_column = "status"          # optional filter
where_value = "translated"

[source.ledger]                  # extractor = "ledger" only
from = "research/engine-map/engine_map.tsv"   # one of the upstream files, with address and name columns
```

The `ledger` extractor makes one row per function: key `f` and the address as eight lower-case hex digits, title
the function's name or, when it has none, its address (`0x00401020`); lines starting with `#` are skipped. With
`items_from` on the group, every function is an item.

A list is TSV with the header `key<TAB>title`. `tally check` fails when a list's SHA-256 is not its pin, when an
item refers to a row the list does not have, and when a row has no item (add one with status missing, n/a or
deferred). `tally refs fetch` downloads the upstream files at the commit (with `curl`), checks their hashes and,
for `tsv` and `ledger`, builds the list again (`--update` writes it and prints the new pin).
