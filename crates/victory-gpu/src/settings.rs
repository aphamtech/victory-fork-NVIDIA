//! The performance profile: the layer's measured speed switches, by tier.
//!
//! Every switch is still read at the place that uses it, and every one of
//! them can still be set by hand (its own variable wins whatever tier is in
//! force). What the profile changes is only the default they take when
//! nothing sets them.
//!
//! `VICTORY_PERF` picks the tier (0, 1 or 2; 2 unless the config file's
//! `[graphics] performance` asks another, which [`set_performance`] passes
//! in):
//!
//! - **0**: every switch at the default its own code documents, which for
//!   all of those below is off: how the layer ran before the profile.
//! - **1**: the switches that change nothing the title can see. Copies of
//!   title memory are made in place, with the kernel's copy behind them for
//!   a page that is not there (`VICTORY_COPY_PLAIN=1`); a texture refresh
//!   no longer waits while the title remaps memory
//!   (`VICTORY_TEXTURE_NO_WAIT=1`).
//! - **2** (the default): also the ones that overlap the card with the
//!   title's own work, where a 3D scene's frame time goes. Submissions fly
//!   ahead of the title (`VICTORY_LATE_WAIT=3`, which brings the relay),
//!   prepared and sent on a thread of ours (`VICTORY_ASYNC_SUBMIT=1`),
//!   fetches go to a copy queue (`VICTORY_QUEUE2=1`), indirect records are
//!   clamped in windows (`VICTORY_INDIRECT_WINDOW=1`,
//!   `VICTORY_WINDOW_JUMP=1`), and a submission may send more of the
//!   title's changed bytes before the flights are settled
//!   (`VICTORY_MERGE_RUNS=4096`).
//! - **3**: tier 2 plus the two whose cost shows only on the title's side
//!   of the frame. The title's changed bytes are joined over small gaps
//!   under flights' writes (`VICTORY_MERGE_GAP=64`), which settles the
//!   flights less; NOT exact, read the comment where it is read (a byte a
//!   shader of a flight in the air wrote inside a gap can be lost — the one
//!   switch in any tier that gives up correctness). And texture levels are
//!   in the card's memory only while the title holds them
//!   (`VICTORY_LEVELS_ON_DEMAND=1`): mostly memory (3 GB less on the test
//!   machine), and less texture work a frame, but a level can arrive a
//!   frame later.
//!
//! Each switch says at its own definition what it assumes, and those
//! assumptions are about the title writing memory between two of its
//! submissions (the console's card reads title memory when it runs a list,
//! later than the submission too). Unit tests run at tier 0, as they always
//! have.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use victory_core::{DEFAULT_PERFORMANCE, PERFORMANCE_TIERS};

/// The tier nothing has asked for yet.
const UNSET: u8 = u8::MAX;

/// The tier the config file asked for.
static ASKED: AtomicU8 = AtomicU8::new(UNSET);

/// Sets the tier the config file asked for (`[graphics] performance`);
/// `VICTORY_PERF` still wins over it. Call before the title starts.
pub fn set_performance(tier: u8) {
    ASKED.store(tier.min(PERFORMANCE_TIERS), Ordering::Relaxed);
}

/// The tier in force: `VICTORY_PERF` if it is set, else [`set_performance`]'s
/// if it was called, else [`DEFAULT_PERFORMANCE`]. Tests take 0: the
/// switches they exercise are the ones their own defaults document.
pub fn performance() -> u8 {
    static TIER: OnceLock<u8> = OnceLock::new();
    *TIER.get_or_init(|| {
        let asked = std::env::var("VICTORY_PERF")
            .ok()
            .and_then(|v| v.trim().parse::<u8>().ok());
        if let Some(tier) = asked {
            return tier.min(PERFORMANCE_TIERS);
        }
        if cfg!(test) {
            return 0;
        }
        match ASKED.load(Ordering::Relaxed) {
            UNSET => DEFAULT_PERFORMANCE,
            tier => tier,
        }
    })
}

/// The environment's value of switch `name`, trimmed; none when it is
/// unset or empty.
fn asked(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

/// Whether switch `name` is on: an explicit `1`, `true` or `on` turns it on
/// and any other explicit value off; unset, it is on from tier `tier` up.
pub fn on(name: &str, tier: u8) -> bool {
    switch_on(asked(name).as_deref(), tier, performance())
}

/// [`on`], for a value and a tier already read.
fn switch_on(value: Option<&str>, tier: u8, tier_now: u8) -> bool {
    match value {
        Some("1" | "true" | "on") => true,
        Some(_) => false,
        None => tier_now >= tier,
    }
}

/// A switch that takes a number: the environment's, else `profile` from
/// tier `tier` up, else none.
pub fn number(name: &str, tier: u8, profile: u64) -> Option<u64> {
    match asked(name) {
        Some(value) => value.parse().ok(),
        None => (performance() >= tier).then_some(profile),
    }
}

/// A switch that takes a word or a level (`1`, `2`, `alt`, ...): the
/// environment's, else `profile` from tier `tier` up, else none.
pub fn word(name: &str, tier: u8, profile: &str) -> Option<String> {
    match asked(name) {
        Some(value) => Some(value),
        None => (performance() >= tier).then(|| profile.to_owned()),
    }
}

/// The switches the profile knows: the variable, the tier that turns it on,
/// and the value it takes then (none for a plain on/off switch).
const SWITCHES: &[(&str, u8, Option<&str>)] = &[
    ("VICTORY_COPY_PLAIN", 1, None),
    ("VICTORY_TEXTURE_NO_WAIT", 1, None),
    ("VICTORY_LATE_WAIT", 2, Some("3")),
    ("VICTORY_ASYNC_SUBMIT", 2, None),
    ("VICTORY_QUEUE2", 2, Some("1")),
    ("VICTORY_INDIRECT_WINDOW", 2, None),
    ("VICTORY_WINDOW_JUMP", 2, Some("1")),
    ("VICTORY_MERGE_RUNS", 2, Some("4096")),
    ("VICTORY_MERGE_GAP", 3, Some("64")),
    ("VICTORY_LEVELS_ON_DEMAND", 3, None),
];

/// What the profile has in force, for the run's log: the tier and every
/// switch it turned on (or found set), with its value.
pub fn summary() -> String {
    let in_force: Vec<String> = SWITCHES
        .iter()
        .filter_map(|&(name, tier, profile)| match profile {
            Some(value) => word(name, tier, value).map(|v| format!("{name}={v}")),
            None => on(name, tier).then(|| name.to_owned()),
        })
        .collect();
    if in_force.is_empty() {
        return "off: every switch at its own default (VICTORY_PERF=0)".to_owned();
    }
    format!(
        "tier {} of {PERFORMANCE_TIERS} (VICTORY_PERF): {}",
        performance(),
        in_force.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tests_run_at_tier_0() {
        assert_eq!(performance(), 0);
        assert!(!on("VICTORY_NOT_SET_ANYWHERE", 1));
        assert_eq!(number("VICTORY_NOT_SET_ANYWHERE", 2, 64), None);
        assert_eq!(word("VICTORY_NOT_SET_ANYWHERE", 2, "3"), None);
    }

    #[test]
    fn an_explicit_value_wins_over_the_tier() {
        assert!(switch_on(Some("1"), 2, 0));
        assert!(switch_on(Some("true"), 2, 0));
        assert!(!switch_on(Some("0"), 1, 2));
        assert!(!switch_on(Some("off"), 1, 2));
        assert!(!switch_on(None, 2, 1));
        assert!(switch_on(None, 2, 2));
        assert!(switch_on(None, 1, 2));
    }
}
