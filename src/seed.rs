//! Cold-start seeding: replay existing shell history into the frecency DB on
//! install, so ccd works immediately instead of after weeks of use.
//!
//! Two-tier relative-cd resolution (most cds are relative, e.g. `cd src`):
//!   1. cross-reference within history: relative `cd foo` + an absolute
//!      `cd ~/dev/foo` elsewhere => known mapping (high confidence)
//!   2. bounded $HOME filesystem scan: unique basename => credit it;
//!      ambiguous => shallowest; missing => dropped
//!
//! Seed visits get a discounted weight so real logged usage overtakes guesses.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::store::{now, Store};

const SEED_WEIGHT: f64 = 0.5; // discounted vs a real logged visit (1.0)
const SEED_SPAN_DAYS: f64 = 30.0; // recency window when timestamps are absent
const SCAN_MAX_DEPTH: usize = 5;

struct CdEvent {
    arg: String,
    epoch: Option<u64>,
    order: usize, // position in history; higher = more recent
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_else(|_| ".".into())
}

// ---------- history parsing ----------

fn parse_zsh(path: &Path, out: &mut Vec<CdEvent>) {
    let Ok(raw) = fs::read_to_string(path) else {
        return;
    };
    for line in raw.lines() {
        // EXTENDED_HISTORY: ": <epoch>:<dur>;<cmd>"
        let (epoch, cmd) = if let Some(rest) = line.strip_prefix(": ") {
            match rest.split_once(';') {
                Some((meta, c)) => {
                    let ep = meta.split(':').next().and_then(|s| s.trim().parse().ok());
                    (ep, c)
                }
                None => (None, line),
            }
        } else {
            (None, line)
        };
        push_if_cd(cmd, epoch, out);
    }
}

fn parse_bash(path: &Path, out: &mut Vec<CdEvent>) {
    let Ok(raw) = fs::read_to_string(path) else {
        return;
    };
    let mut pending_epoch: Option<u64> = None;
    for line in raw.lines() {
        // HISTTIMEFORMAT writes "#<epoch>" on its own line before each command
        if let Some(rest) = line.strip_prefix('#') {
            if let Ok(ep) = rest.trim().parse::<u64>() {
                pending_epoch = Some(ep);
                continue;
            }
        }
        push_if_cd(line, pending_epoch.take(), out);
    }
}

fn parse_fish(path: &Path, out: &mut Vec<CdEvent>) {
    let Ok(raw) = fs::read_to_string(path) else {
        return;
    };
    let mut cur_cmd: Option<String> = None;
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("- cmd: ") {
            if let Some(c) = cur_cmd.take() {
                push_if_cd(&c, None, out);
            }
            cur_cmd = Some(rest.to_string());
        } else if let Some(rest) = line.trim_start().strip_prefix("when: ") {
            if let Some(c) = cur_cmd.take() {
                let ep = rest.trim().parse().ok();
                push_if_cd(&c, ep, out);
            }
        }
    }
    if let Some(c) = cur_cmd.take() {
        push_if_cd(&c, None, out);
    }
}

fn push_if_cd(cmd: &str, epoch: Option<u64>, out: &mut Vec<CdEvent>) {
    let Some(mut rest) = strip_cd_invocation(cmd) else {
        return;
    };
    rest = rest.trim_start();
    if let Some(after) = rest.strip_prefix("--") {
        if after.chars().next().is_some_and(char::is_whitespace) {
            rest = after.trim_start();
        }
    }
    let Some(arg) = first_shell_word(rest) else {
        return;
    };
    let arg = arg.trim();
    if arg.is_empty() {
        return;
    }
    let order = out.len();
    out.push(CdEvent {
        arg: arg.to_string(),
        epoch,
        order,
    });
}

fn strip_cd_invocation(cmd: &str) -> Option<&str> {
    let cmd = cmd.trim_start();
    for prefix in ["builtin cd", "command cd", "cd"] {
        if let Some(rest) = cmd.strip_prefix(prefix) {
            if rest.chars().next().is_some_and(char::is_whitespace) {
                return Some(rest.trim_start());
            }
        }
    }
    None
}

fn first_shell_word(input: &str) -> Option<String> {
    let mut out = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in input.chars() {
        if escaped {
            out.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                out.push(ch);
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            ';' => break,
            c if c.is_whitespace() => {
                if out.is_empty() {
                    continue;
                }
                break;
            }
            _ => out.push(ch),
        }
    }
    if escaped {
        out.push('\\');
    }
    let out = out.trim().to_string();
    (!out.is_empty()).then_some(out)
}

fn collect_events() -> Vec<CdEvent> {
    let h = home();
    let mut out = Vec::new();
    // zsh: honor $HISTFILE if set
    let zsh = std::env::var("HISTFILE")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&h).join(".zsh_history"));
    parse_zsh(&zsh, &mut out);
    parse_bash(&PathBuf::from(&h).join(".bash_history"), &mut out);
    parse_fish(
        &PathBuf::from(&h).join(".local/share/fish/fish_history"),
        &mut out,
    );
    out
}

// ---------- resolution ----------

fn expand(arg: &str, h: &str) -> Option<String> {
    if arg.starts_with('/') {
        Some(arg.to_string())
    } else if let Some(rest) = arg.strip_prefix("~/") {
        Some(format!("{}/{}", h, rest))
    } else if arg == "~" {
        Some(h.to_string())
    } else {
        arg.strip_prefix("$HOME/")
            .map(|rest| format!("{}/{}", h, rest))
    }
}

fn canon_dir(p: &str) -> Option<String> {
    let c = fs::canonicalize(p).ok()?;
    if c.is_dir() {
        Some(c.to_string_lossy().into_owned())
    } else {
        None
    }
}

/// Bounded $HOME scan -> basename => list of full paths.
fn scan_home(h: &str) -> HashMap<String, Vec<String>> {
    const SKIP: &[&str] = &[
        "node_modules",
        ".git",
        "Library",
        ".cache",
        ".Trash",
        ".npm",
        ".cargo",
        "venv",
        ".venv",
        "__pycache__",
        ".vscode",
    ];
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    let mut stack = vec![(PathBuf::from(h), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if depth >= SCAN_MAX_DEPTH {
            continue;
        }
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for ent in rd.flatten() {
            let Ok(ft) = ent.file_type() else { continue };
            if !ft.is_dir() {
                continue;
            }
            let name = ent.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || SKIP.contains(&name.as_str()) {
                continue;
            }
            let full = ent.path().to_string_lossy().into_owned();
            map.entry(name).or_default().push(full.clone());
            stack.push((ent.path(), depth + 1));
        }
    }
    map
}

fn depth_of(p: &str) -> usize {
    p.bytes().filter(|&b| b == b'/').count()
}

// ---------- driver ----------

pub struct SeedReport {
    pub events: usize,
    pub resolved: usize,
    pub unique_dirs: usize,
    pub paths: Vec<String>,
}

pub fn seed(dry_run: bool) -> std::io::Result<SeedReport> {
    let cfg = Config::frozen();
    let h = home();
    let events = collect_events();

    // tier 1: cross-reference map from absolute/~ cds that exist today
    let mut crossref: HashMap<String, String> = HashMap::new();
    for e in &events {
        if let Some(p) = expand(&e.arg, &h) {
            if let Some(c) = canon_dir(&p) {
                let base = c.rsplit('/').next().unwrap_or(&c).to_string();
                crossref.entry(base).or_insert(c);
            }
        }
    }
    // tier 2: filesystem scan
    let fsmap = scan_home(&h);

    let resolve = |arg: &str| -> Option<String> {
        if arg == "-" || arg == "." || arg == ".." || arg.starts_with("..") {
            return None;
        }
        if let Some(p) = expand(arg, &h) {
            return canon_dir(&p);
        }
        let name = arg.rsplit('/').next().unwrap_or(arg);
        if let Some(c) = crossref.get(name) {
            return Some(c.clone());
        }
        match fsmap.get(name) {
            Some(v) if v.len() == 1 => canon_dir(&v[0]),
            Some(v) if v.len() > 1 => {
                // ambiguous -> shallowest (closest to $HOME)
                let best = v.iter().min_by_key(|p| depth_of(p))?;
                canon_dir(best)
            }
            _ => None,
        }
    };

    // accumulate decayed seed mass per directory
    let t_now = now();
    let total = events.len().max(1) as f64;
    let mut mass: HashMap<String, f64> = HashMap::new();
    let mut resolved = 0usize;
    for e in &events {
        let Some(path) = resolve(&e.arg) else {
            continue;
        };
        resolved += 1;
        let age_secs = match e.epoch {
            Some(ep) if ep <= t_now => (t_now - ep) as f64,
            // no timestamp: use history order as a recency proxy
            _ => {
                let frac = e.order as f64 / total; // 0 oldest .. ~1 newest
                (1.0 - frac) * SEED_SPAN_DAYS * 86_400.0
            }
        };
        let contrib = SEED_WEIGHT * 2f64.powf(-age_secs / cfg.h_secs);
        *mass.entry(path).or_insert(0.0) += contrib;
    }

    let unique = mass.len();
    let mut paths: Vec<String> = mass.keys().cloned().collect();
    paths.sort();
    if !dry_run {
        // merge into store as-of-now (t_last = now, score = seeded f_eff)
        let mut store = Store::load_locked();
        if !store.is_locked() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "database is busy",
            ));
        }
        for (path, m) in mass {
            let entry = store.map.entry(path).or_insert(crate::store::Entry {
                score: 0.0,
                last: t_now,
            });
            // decay any existing mass to now, then add the seed contribution
            let dt = t_now.saturating_sub(entry.last) as f64;
            entry.score = entry.score * 2f64.powf(-dt / cfg.h_secs) + m;
            entry.last = t_now;
        }
        store.save()?;
    }

    Ok(SeedReport {
        events: events.len(),
        resolved,
        unique_dirs: unique,
        paths,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_cd_paths() {
        let mut out = Vec::new();
        push_if_cd("cd \"My Projects/app one\"", Some(10), &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].arg, "My Projects/app one");
        assert_eq!(out[0].epoch, Some(10));
    }

    #[test]
    fn parses_builtin_and_escaped_paths() {
        let mut out = Vec::new();
        push_if_cd("builtin cd dev/my\\ app", None, &mut out);
        push_if_cd("command cd ~/Downloads", None, &mut out);
        assert_eq!(out[0].arg, "dev/my app");
        assert_eq!(out[1].arg, "~/Downloads");
    }

    #[test]
    fn ignores_non_cd_and_empty_cd() {
        let mut out = Vec::new();
        push_if_cd("echo cd Downloads", None, &mut out);
        push_if_cd("cd", None, &mut out);
        assert!(out.is_empty());
    }
}
