// SPDX-License-Identifier: GPL-3.0-only
//! The state of each item, from what the catalogue declares, the tests that tag it and the test results.

use crate::catalogue::{Catalogue, Declared, Item};
use crate::results::{Index, PLATFORMS, Results};
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

/// The outcome of one test on one OS (see [`Index::outcome`]).
pub type OutcomeOf<'a> = &'a dyn Fn(&TestRef, &str) -> Option<bool>;

/// The state of one item.
#[must_use]
pub fn of(item: &Item, tests: &BTreeSet<TestRef>, results: Option<OutcomeOf<'_>>) -> ItemState {
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
                    if let Some(outcome) = results {
                        for os in PLATFORMS {
                            let outcomes: Vec<Option<bool>> =
                                tests.iter().map(|t| outcome(t, os)).collect();
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
    let empty = BTreeSet::new();
    let index = results.map(Index::new);
    let packages = std::cell::RefCell::new(BTreeMap::new());
    let outcome = |t: &TestRef, os: &str| {
        let index = index.as_ref()?;
        let (package, file) = packages
            .borrow_mut()
            .entry(t.file.clone())
            .or_insert_with(|| cat.package_for(&t.file))
            .clone();
        index.outcome(&package, &file, &t.function, os)
    };
    let outcome: Option<OutcomeOf<'_>> = results.map(|_| &outcome as OutcomeOf<'_>);
    cat.items()
        .map(|i| of(i, tags.get(&i.id).unwrap_or(&empty), outcome))
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
    /// Missing items that work another way (evidence `works`); they stay in `missing` too.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub works: usize,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(n: &usize) -> bool {
    *n == 0
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
