// SPDX-License-Identifier: GPL-3.0-only
// Writes the ids module of tally's own catalogue, the same way a repository's build.rs does.
include!("src/idsgen.rs");

fn main() {
    tally_ids_gen::build("catalogue");
}
