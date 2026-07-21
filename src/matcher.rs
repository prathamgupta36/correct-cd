//! Fuzzy matching: subsequence (abbreviations) + OSA edit distance (typos).
//! `fuzzy = max(subseq, edit)`. Direct port of prototype/ranker.py.

use crate::config::Config;

/// Optimal String Alignment (restricted Damerau-Levenshtein). Handles
/// insert/delete/substitute + adjacent transposition (`Donwloads`).
pub fn osa_distance(a: &[char], b: &[char]) -> usize {
    let (la, lb) = (a.len(), b.len());
    if la == 0 {
        return lb;
    }
    if lb == 0 {
        return la;
    }
    let mut prev2 = vec![0usize; lb + 1];
    let mut prev: Vec<usize> = (0..=lb).collect();
    let mut cur = vec![0usize; lb + 1];
    for i in 1..=la {
        cur[0] = i;
        for j in 1..=lb {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            let mut v = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                v = v.min(prev2[j - 2] + 1);
            }
            cur[j] = v;
        }
        // rotate buffers: prev2 <- prev, prev <- cur, reuse old prev2 as cur
        std::mem::swap(&mut prev2, &mut prev);
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[lb]
}

/// fzf-style subsequence score in [0,1]. 0 if `q` is not a subsequence.
pub fn subseq_score(q: &str, base: &str, cfg: &Config) -> f64 {
    let ql: Vec<char> = q.to_lowercase().chars().collect();
    if ql.is_empty() {
        return 0.0;
    }
    let base_chars: Vec<char> = base.chars().collect();
    let bl: Vec<char> = base.to_lowercase().chars().collect();

    // greedy leftmost subsequence match
    let mut positions: Vec<usize> = Vec::with_capacity(ql.len());
    let mut j = 0usize;
    for &ch in &ql {
        let mut found: Option<usize> = None;
        let mut k = j;
        while k < bl.len() {
            if bl[k] == ch {
                found = Some(k);
                break;
            }
            k += 1;
        }
        match found {
            Some(p) => {
                positions.push(p);
                j = p + 1;
            }
            None => return 0.0,
        }
    }

    let m = ql.len() as f64;
    let first = positions[0];
    let last = *positions.last().unwrap();
    let span = (last - first + 1) as f64;
    let n = bl.len().max(1) as f64;
    let mut score = 1.0 - cfg.gamma_gap * (span - m) / n - cfg.gamma_pos * (first as f64) / n;

    // acronym/initialism bonus: scales with # of matched chars on a word boundary
    let mut at_boundary = 0usize;
    for &p in &positions {
        let sep = p == 0 || matches!(bl[p - 1], '-' | '_' | '.' | ' ' | '/');
        let camel = p > 0
            && p < base_chars.len()
            && base_chars[p].is_uppercase()
            && base_chars[p - 1].is_lowercase();
        if sep || camel {
            at_boundary += 1;
        }
    }
    if at_boundary > 0 {
        score += cfg.boundary_base + cfg.acronym_scale * (at_boundary as f64 / m);
    }
    score.clamp(0.0, 1.0)
}

/// Edit-distance similarity in [0,1].
pub fn edit_score(q: &str, base: &str) -> f64 {
    let ql: Vec<char> = q.to_lowercase().chars().collect();
    let bl: Vec<char> = base.to_lowercase().chars().collect();
    let d = osa_distance(&ql, &bl);
    let denom = ql.len().max(bl.len()).max(1) as f64;
    1.0 - d as f64 / denom
}

pub fn fuzzy_score(q: &str, base: &str, cfg: &Config) -> f64 {
    subseq_score(q, base, cfg).max(edit_score(q, base))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osa_handles_adjacent_transposition() {
        let a: Vec<char> = "donwloads".chars().collect();
        let b: Vec<char> = "downloads".chars().collect();
        assert_eq!(osa_distance(&a, &b), 1);
    }

    #[test]
    fn subsequence_scores_abbreviations() {
        let cfg = Config::frozen();
        assert!(subseq_score("dwn", "Downloads", &cfg) >= cfg.min_match);
        assert!(subseq_score("wn", "web-node", &cfg) >= cfg.min_match);
    }

    #[test]
    fn fuzzy_accepts_typo() {
        let cfg = Config::frozen();
        assert!(fuzzy_score("documnets", "Documents", &cfg) >= cfg.min_match);
    }
}
