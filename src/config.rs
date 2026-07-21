//! Frozen ranking constants.
//!
//! These were locked via A/B testing in `prototype/` (see prototype/eval.py).
//! Do NOT hand-tune them here — change them in the prototype, re-run the eval,
//! and port the winning numbers back. `H` and `MIN_MATCH` and `alpha` in
//! particular are evidence-backed; `alpha` is the one still worth revisiting
//! against real replayed history.

pub struct Config {
    pub alpha: f64,         // fuzzy exponent (match-correctness weight)
    pub beta: f64,          // frecency exponent (habit weight)
    pub h_secs: f64,        // frecency half-life, in seconds
    pub k: f64,             // frecency saturation constant
    pub min_match: f64,     // hard gate: suggest only if fuzzy >= this
    pub gamma_gap: f64,     // subsequence gap penalty
    pub gamma_pos: f64,     // subsequence non-prefix penalty
    pub boundary_base: f64, // floor bonus if any matched char is a boundary
    pub acronym_scale: f64, // extra bonus scaling with # of word-initials
    pub child_boost: f64,   // multiplier for a direct child of cwd
    pub child_margin: f64,  // child boost only applies within this of best fuzzy
}

impl Config {
    pub const fn frozen() -> Config {
        Config {
            alpha: 1.5,
            beta: 1.0,
            h_secs: 14.0 * 86_400.0,
            k: 4.0,
            min_match: 0.70,
            gamma_gap: 0.7,
            gamma_pos: 0.3,
            boundary_base: 0.1,
            acronym_scale: 0.25,
            child_boost: 1.75,
            child_margin: 0.05,
        }
    }
}
