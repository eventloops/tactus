//! Extended notes: `docs/internals/engine/topology.md`

#![cfg_attr(not(test), allow(dead_code))]

pub mod attempt;
pub mod candidate;
pub mod closure;
pub mod coverage;
pub mod create;
pub mod dispatch;
pub mod emit;
pub mod finalize;
pub mod identity;
pub mod integrate;
pub mod ledger;
pub mod prelock;
pub mod reachability;
pub mod repair;
pub mod report;
pub mod seams;
pub mod select;
pub mod settle;
pub(crate) mod startup;

pub mod preflight;
pub mod recover;
pub mod run;

#[cfg(test)]
mod scaffold;
