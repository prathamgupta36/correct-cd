//! Ranking: final = fuzzy^alpha * frecency_norm^beta, gated by MIN_MATCH,
//! with child-of-cwd used only as a tiebreaker among near-best matches.
//! Direct port of prototype/ranker.py `rank()`.

use crate::config::Config;
use crate::matcher::fuzzy_score;
use crate::store::Store;

pub struct Cand {
    pub path: String,
    #[allow(dead_code)] // kept for --list/debug output and future ghost-text UI
    pub fuzzy: f64,
    pub f_eff: f64,
    pub score: f64,
}

fn basename(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
}

fn components(path: &str) -> Vec<&str> {
    path.split('/').filter(|c| !c.is_empty()).collect()
}

/// For 'proj/src'-style fragments: match segments as an ordered subsequence
/// over path components. Returns the last segment's score, or None if any
/// earlier segment can't clear the gate.
fn multiseg_fuzzy(qsegs: &[&str], path: &str, cfg: &Config) -> Option<f64> {
    let comps = components(path);
    let mut ci = 0usize;
    let mut last = 0.0f64;
    for (qi, qs) in qsegs.iter().enumerate() {
        let mut best_sc = 0.0f64;
        let mut best_k: Option<usize> = None;
        let mut k = ci;
        while k < comps.len() {
            let sc = fuzzy_score(qs, comps[k], cfg);
            if sc > best_sc {
                best_sc = sc;
                best_k = Some(k);
            }
            k += 1;
        }
        let is_last = qi == qsegs.len() - 1;
        if !is_last && best_sc < cfg.min_match {
            return None;
        }
        let k = best_k?;
        ci = k + 1;
        last = best_sc;
    }
    Some(last)
}

fn is_direct_child(path: &str, cwd: &str) -> bool {
    !cwd.is_empty()
        && path.len() > cwd.len()
        && path.starts_with(cwd)
        && path.as_bytes()[cwd.len()] == b'/'
        && !path[cwd.len() + 1..].contains('/')
}

pub fn rank(fragment: &str, cwd: &str, store: &Store, cfg: &Config, now: u64) -> Vec<Cand> {
    let qsegs: Vec<&str> = fragment.split('/').filter(|s| !s.is_empty()).collect();
    let multiseg = qsegs.len() > 1;

    // pass 1: gate-passing candidates with their fuzzy scores
    let mut cands: Vec<(String, f64)> = Vec::new();
    for path in store.map.keys() {
        let fz = if multiseg {
            match multiseg_fuzzy(&qsegs, path, cfg) {
                Some(v) => v,
                None => continue,
            }
        } else {
            fuzzy_score(fragment, basename(path), cfg)
        };
        if fz < cfg.min_match {
            continue;
        }
        cands.push((path.clone(), fz));
    }
    if cands.is_empty() {
        return Vec::new();
    }
    let max_fuzzy = cands.iter().map(|(_, f)| *f).fold(0.0f64, f64::max);

    // pass 2: score; child boost only breaks ties among near-best matches
    let mut out: Vec<Cand> = Vec::with_capacity(cands.len());
    for (path, fz) in cands {
        let e = &store.map[&path];
        let dt = now.saturating_sub(e.last) as f64;
        let f_eff = e.score * 2f64.powf(-dt / cfg.h_secs);
        let fnorm = f_eff / (f_eff + cfg.k);
        let mut score = fz.powf(cfg.alpha) * fnorm.powf(cfg.beta);
        if is_direct_child(&path, cwd) && fz >= max_fuzzy - cfg.child_margin {
            score *= cfg.child_boost;
        }
        out.push(Cand {
            path,
            fuzzy: fz,
            f_eff,
            score,
        });
    }

    // deterministic tie-break: score, then f_eff, then shorter path, then lex
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap()
            .then(b.f_eff.partial_cmp(&a.f_eff).unwrap())
            .then(a.path.len().cmp(&b.path.len()))
            .then(a.path.cmp(&b.path))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Entry, Store};
    use std::path::PathBuf;

    fn store_with(rows: &[(&str, f64, u64)]) -> Store {
        let mut store = Store::load_from(PathBuf::from("/tmp/ccd-rank-test.tsv"));
        store.map.clear();
        for (path, score, last) in rows {
            store.map.insert(
                (*path).to_string(),
                Entry {
                    score: *score,
                    last: *last,
                },
            );
        }
        store
    }

    #[test]
    fn ranks_abbreviation_by_match_and_frecency() {
        let cfg = Config::frozen();
        let store = store_with(&[
            ("/home/u/Downloads", 5.0, 100),
            ("/home/u/Documents", 1.0, 100),
        ]);
        let ranked = rank("dwn", "/home/u", &store, &cfg, 100);
        assert_eq!(ranked[0].path, "/home/u/Downloads");
    }

    #[test]
    fn boosts_direct_child_only_for_near_best_match() {
        let cfg = Config::frozen();
        let store = store_with(&[
            ("/home/u/project/src", 1.0, 100),
            ("/home/u/other/source", 1.0, 100),
        ]);
        let ranked = rank("src", "/home/u/project", &store, &cfg, 100);
        assert_eq!(ranked[0].path, "/home/u/project/src");
    }

    #[test]
    fn multisegment_query_requires_ordered_components() {
        let cfg = Config::frozen();
        let store = store_with(&[
            ("/home/u/dev/web-node/src", 2.0, 100),
            ("/home/u/src/web-node", 10.0, 100),
        ]);
        let ranked = rank("dev/src", "/home/u", &store, &cfg, 100);
        assert_eq!(ranked[0].path, "/home/u/dev/web-node/src");
    }
}
