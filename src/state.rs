// SPDX-License-Identifier: GPL-3.0-only
//! The state of each item, from what the catalogue declares, the tests that tag it and the test results.

use crate::catalogue::{Catalogue, Declared, Item};
use crate::results::{self, PLATFORMS, Results};
use crate::scan::{Site, TestRef};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// The states tally reports, from least to most done. `Gated`, `NotApplicable`, `Deferred` and `Retired` are
/// outside that line: gated items are implemented but can never be covered by CI; the last three are not counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Missing,
    Implemented,
    Tagged,
    Passing,
    Covered,
    Gated,
    #[serde(rename = "n/a")]
    NotApplicable,
    Deferred,
    Retired,
}

impl State {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            State::Missing => "missing",
            State::Implemented => "implemented",
            State::Tagged => "tagged",
            State::Passing => "passing",
            State::Covered => "covered",
            State::Gated => "gated",
            State::NotApplicable => "n/a",
            State::Deferred => "deferred",
            State::Retired => "retired",
        }
    }

    /// Counted in the denominator.
    #[must_use]
    pub fn counted(self) -> bool {
        !matches!(
            self,
            State::NotApplicable | State::Deferred | State::Retired
        )
    }

    /// Implemented in any way (gated included).
    #[must_use]
    pub fn implemented(self) -> bool {
        matches!(
            self,
            State::Implemented | State::Tagged | State::Passing | State::Covered | State::Gated
        )
    }
}

/// The computed state of one item.
#[derive(Clone, Debug)]
pub struct ItemState {
    pub id: String,
    pub state: State,
    /// The tests that tag it (ignored tests left out).
    pub tests: Vec<TestRef>,
    /// Per OS: true when a tagged test passed there and none failed.
    pub platforms: BTreeMap<String, bool>,
}

/// The tests that tag each id (ignored tests and calls outside tests left out).
#[must_use]
pub fn tags(sites: &[Site]) -> BTreeMap<String, BTreeSet<TestRef>> {
    let mut map: BTreeMap<String, BTreeSet<TestRef>> = BTreeMap::new();
    for s in sites.iter().filter(|s| s.is_test && !s.ignored) {
        let Some(t) = &s.test else { continue };
        for id in &s.ids {
            map.entry(id.clone()).or_default().insert(t.clone());
        }
    }
    map
}

/// The state of one item.
#[must_use]
pub fn of(
    item: &Item,
    tests: &BTreeSet<TestRef>,
    results: Option<&Results>,
    package: &str,
) -> ItemState {
    let mut platforms = BTreeMap::new();
    let state = if item.retired.is_some() {
        State::Retired
    } else {
        match item.declared {
            Declared::NotApplicable => State::NotApplicable,
            Declared::Deferred => State::Deferred,
            Declared::Missing => State::Missing,
            Declared::Partial | Declared::Implemented => {
                if item.gated.is_some() {
                    State::Gated
                } else if tests.is_empty() {
                    State::Implemented
                } else {
                    if let Some(r) = results {
                        for os in PLATFORMS {
                            let outcomes: Vec<Option<bool>> = tests
                                .iter()
                                .map(|t| results::outcome(r, package, t, os))
                                .collect();
                            let pass =
                                outcomes.contains(&Some(true)) && !outcomes.contains(&Some(false));
                            platforms.insert(os.to_string(), pass);
                        }
                    }
                    if !platforms.is_empty() && platforms.values().all(|p| *p) {
                        State::Covered
                    } else if platforms.values().any(|p| *p) {
                        State::Passing
                    } else {
                        State::Tagged
                    }
                }
            }
        }
    };
    ItemState {
        id: item.id.clone(),
        state,
        tests: tests.iter().cloned().collect(),
        platforms,
    }
}

/// The state of every item, in catalogue order.
#[must_use]
pub fn all(cat: &Catalogue, sites: &[Site], results: Option<&Results>) -> Vec<ItemState> {
    let tags = tags(sites);
    let package = cat.package();
    let empty = BTreeSet::new();
    cat.items()
        .map(|i| of(i, tags.get(&i.id).unwrap_or(&empty), results, &package))
        .collect()
}

/// Counts of a set of items.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct Counts {
    /// Items in the denominator (not n/a, deferred or retired).
    pub total: usize,
    pub missing: usize,
    /// Implemented in any way, gated included.
    pub implemented: usize,
    /// Tagged by at least one test (tagged, passing or covered).
    pub tagged: usize,
    /// Passed somewhere (passing or covered).
    pub passing: usize,
    /// Passed on Linux, Windows and macOS.
    pub covered: usize,
    pub gated: usize,
    pub not_applicable: usize,
    pub deferred: usize,
}

impl Counts {
    pub fn add(&mut self, s: State) {
        match s {
            State::NotApplicable => self.not_applicable += 1,
            State::Deferred => self.deferred += 1,
            State::Retired => {}
            _ => {
                self.total += 1;
                if s == State::Missing {
                    self.missing += 1;
                }
                if s.implemented() {
                    self.implemented += 1;
                }
                if matches!(s, State::Tagged | State::Passing | State::Covered) {
                    self.tagged += 1;
                }
                if matches!(s, State::Passing | State::Covered) {
                    self.passing += 1;
                }
                if s == State::Covered {
                    self.covered += 1;
                }
                if s == State::Gated {
                    self.gated += 1;
                }
            }
        }
    }
}
