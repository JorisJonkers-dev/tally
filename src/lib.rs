// SPDX-License-Identifier: GPL-3.0-only
//! tally: a feature catalogue per repository, typed test tags, and reports of what is implemented, tested and
//! covered on Linux, Windows and macOS. See README.md.

pub mod catalogue;
pub mod check;
pub mod cli;
pub mod doc;
pub mod refs;
pub mod report;
pub mod results;
pub mod scan;
pub mod sha256;
pub mod state;

/// The ids generator (std only); repositories include a copy of this file from build.rs.
pub mod ids {
    include!("idsgen.rs");
}

/// tally's own ids, for its unit tests.
#[cfg(test)]
pub(crate) mod own {
    include!(concat!(env!("OUT_DIR"), "/tally_ids.rs"));
}
