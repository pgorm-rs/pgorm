#![allow(unused_imports, dead_code)]

//! The identifier render oracle.
//!
//! sqlmap cannot fire in identifier positions: a level-3 boundary cannot close
//! a faithfully double-quoted identifier and attach a payload, so the cases
//! whose input is a name carry few or no scheduled techniques
//! (`security/sqlmap/README.md`). This oracle judges those positions
//! structurally instead. Every public API that renders a caller-supplied name
//! into SQL text is registered (`identifier_oracle/registry*.rs`); each is
//! rendered with every name in a hostile corpus; and each statement is parsed
//! by libpg_query and compared, node for node, with the same statement
//! rendered with a benign name. A name that changed the statement's shape —
//! closed its quote, ended the statement, opened a comment, turned into a
//! keyword — shows up as a difference in the tree, whatever payload a scanner
//! would have needed to find it.
//!
//! The live leg (`identifier_oracle/live.rs`) runs the nastiest names against
//! a real server, so the parser's reading is checked against the server's, and
//! `identifier_oracle/live_capture.rs` holds the sites whose SQL only exists on
//! its way to a server — captured from tokio-postgres's statement log — to the
//! same property as the rest.

pub mod common;
mod identifier_oracle;

use std::collections::{BTreeMap, BTreeSet};

use identifier_oracle::{
    corpus::corpus,
    oracle::{Verdict, judge, judge_against, judge_nul, reference},
    pins::{PINS, pinned},
    registry::sites,
};

/// Every site's benign rendering parses and puts the name exactly where the
/// registry says, and no two sites share an id.
// [spec:pgorm:req:security.ident-oracle+2/test]
#[test]
fn every_site_declares_where_its_name_lands() {
    let sites = sites();
    let mut ids = BTreeSet::new();
    let failures: Vec<String> = sites
        .iter()
        .filter_map(|site| {
            if !ids.insert(site.id) {
                return Some(format!("site id `{}` is registered twice", site.id));
            }
            reference(site)
                .err()
                .map(|err| format!("site `{}` ({}): {err}", site.id, site.api))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} sites misdeclared:\n{}",
        failures.len(),
        sites.len(),
        failures.join("\n")
    );
}

/// Every hostile name at every site either round-trips as exactly the
/// declared identifiers with the statement's shape unchanged, or meets the
/// outcome its policy requires instead — a refusal, the grammar's own
/// rejection of the empty name, a type keyword read as its type. A failure
/// names the site, the name and the structural difference.
// [spec:pgorm:req:security.ident-oracle+2/test]
#[test]
fn every_site_holds_every_hostile_name() {
    let sites = sites();
    let corpus = corpus();
    let mut failures = Vec::new();
    let mut tally: BTreeMap<&str, BTreeMap<Verdict, usize>> = BTreeMap::new();
    for site in &sites {
        let Ok(reference) = reference(site) else {
            continue;
        };
        for hostile in &corpus {
            let outcome = judge_against(site, hostile, (site.render)(&hostile.name), &reference);
            match (outcome, pinned(site.id, hostile.label)) {
                (Ok(verdict), None) => {
                    *tally
                        .entry(site.crate_name())
                        .or_default()
                        .entry(verdict)
                        .or_default() += 1;
                }
                (Err(_), Some(_)) => {}
                (Err(failure), None) => failures.push(failure),
                (Ok(verdict), Some(pin)) => failures.push(format!(
                    "site `{}`, name `{}`: pinned as defect `{}` but now passes ({verdict:?}); \
                     retire the pin",
                    site.id, hostile.label, pin.node
                )),
            }
        }
    }
    eprintln!("verdicts by crate: {tally:#?}");
    assert!(
        failures.is_empty(),
        "{} site × name failures across {} sites and {} names:\n\n{}",
        failures.len(),
        sites.len(),
        corpus.len(),
        failures.join("\n\n")
    );
}

/// A NUL-bearing name is refused by the API, or reaches the statement text
/// where the protocol encoder refuses it, or is escaped into a literal the
/// grammar refuses — whichever the site's policy declares, and never
/// anything that parses.
// [spec:pgorm:req:security.ident-oracle.nul+2/test]
#[test]
fn every_site_keeps_nul_out_of_the_server() {
    let sites = sites();
    let mut by_behaviour: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    let failures: Vec<String> = sites
        .iter()
        .filter_map(|site| match judge_nul(site) {
            Ok(behaviour) => {
                by_behaviour
                    .entry(format!("{behaviour:?}"))
                    .or_default()
                    .push(site.id);
                None
            }
            Err(failure) => Some(failure),
        })
        .collect();
    eprintln!("NUL behaviour by site: {by_behaviour:#?}");
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// Each pinned defect still reproduces exactly as filed. A pin fails the
/// moment its site × name pair starts passing, so a fix cannot land without
/// the pin being retired, and the defect is reported on every run until then.
// [spec:pgorm:req:security.ident-oracle+2/test]
#[test]
fn pinned_identifier_defects_still_reproduce() {
    let sites = sites();
    let corpus = corpus();
    for pin in PINS {
        for label in pin.labels {
            let site = sites
                .iter()
                .find(|site| site.id == pin.site)
                .unwrap_or_else(|| panic!("pin names unregistered site `{}`", pin.site));
            let hostile = corpus
                .iter()
                .find(|hostile| hostile.label == *label)
                .unwrap_or_else(|| panic!("pin names unknown corpus entry `{label}`"));
            match judge(site, hostile) {
                Err(failure) => eprintln!("PINNED DEFECT {}: {failure}", pin.node),
                Ok(verdict) => panic!(
                    "pin `{}` for site `{}`, name `{label}` is stale: it now passes ({verdict:?})",
                    pin.node, pin.site
                ),
            }
        }
    }
}
