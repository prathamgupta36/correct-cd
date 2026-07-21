//! ccd — typo/abbreviation-tolerant cd predictor.
//!
//!   ccd add <path>              record a visit (called by the shell hook)
//!   ccd query <frag> [--cwd D]  print best-matching dir (empty if none)
//!   ccd query <frag> --list     print all gate-passing dirs, best first
//!   ccd seed [--dry-run]        cold-start from existing shell history
//!   ccd init <zsh|bash|fish>    print shell integration to eval

mod config;
mod matcher;
mod rank;
mod seed;
mod store;

use std::path::Path;
use std::process::exit;

use config::Config;
use store::{now, Entry, Store};

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("");
    let rest = &args[2.min(args.len())..];
    match cmd {
        "add" => cmd_add(rest),
        "query" => cmd_query(rest),
        "seed" => cmd_seed(rest),
        "init" => cmd_init(rest),
        "forget" => cmd_forget(rest),
        "prune" => cmd_prune(rest),
        "stats" => cmd_stats(rest),
        "doctor" => cmd_doctor(rest),
        "-h" | "--help" | "help" | "" => usage(0),
        _ => {
            eprintln!("ccd: unknown command '{cmd}'");
            usage(2);
        }
    }
}

fn usage(code: i32) -> ! {
    eprintln!(
        "usage:\n  ccd add <path>\n  ccd query <frag> [--cwd DIR] [--list]\n  \
         ccd seed [--dry-run]\n  ccd init <zsh|bash|fish>\n  ccd stats\n  \
         ccd prune [--dry-run]\n  ccd forget <path>\n  ccd doctor"
    );
    exit(code);
}

/// Split positional args from `--flag[=value]` / `--flag value` options.
fn parse_flags<'a>(args: &'a [String], valued: &[&str]) -> (Vec<&'a str>, Vec<(String, String)>) {
    let mut pos = Vec::new();
    let mut flags = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(f) = a.strip_prefix("--") {
            if let Some((k, v)) = f.split_once('=') {
                flags.push((k.to_string(), v.to_string()));
            } else if valued.contains(&f) && i + 1 < args.len() {
                flags.push((f.to_string(), args[i + 1].clone()));
                i += 1;
            } else {
                flags.push((f.to_string(), String::new()));
            }
        } else {
            pos.push(a.as_str());
        }
        i += 1;
    }
    (pos, flags)
}

fn cmd_add(args: &[String]) {
    let (pos, _) = parse_flags(args, &[]);
    let Some(raw) = pos.first() else { usage(2) };
    // only record real directories, canonicalized (dedupes symlinks)
    let Ok(canon) = std::fs::canonicalize(raw) else {
        return;
    };
    if !canon.is_dir() {
        return;
    }
    let cfg = Config::frozen();
    let mut store = Store::load_locked();
    if !store.is_locked() {
        return;
    }
    store.add(&canon.to_string_lossy(), 1.0, now(), cfg.h_secs);
    let _ = store.save();
}

fn cmd_query(args: &[String]) {
    let (pos, flags) = parse_flags(args, &["cwd"]);
    let Some(fragment) = pos.first() else { exit(1) };
    let cwd = flags
        .iter()
        .find(|(k, _)| k == "cwd")
        .map(|(_, v)| canonical_dir_or_input(v))
        .unwrap_or_default();
    let list = flags.iter().any(|(k, _)| k == "list");
    let complete = flags.iter().any(|(k, _)| k == "complete");

    let mut cfg = Config::frozen();
    if complete {
        cfg.min_match = 0.50;
        cfg.child_margin = 0.20;
    }
    let t = now();
    let mut store = Store::load();
    if complete {
        add_completion_dirs(&mut store, &cwd, t);
    }
    let ranked = rank::rank(fragment, &cwd, &store, &cfg, t);

    if list {
        for c in &ranked {
            if Path::new(&c.path).is_dir() {
                println!("{}", c.path);
            }
        }
        return;
    }
    // top-1: skip stale (deleted) dirs so we never suggest a dead path
    for c in &ranked {
        if Path::new(&c.path).is_dir() {
            println!("{}", c.path);
            return;
        }
    }
    exit(1); // no suggestion -> shell falls through to native behavior
}

fn cmd_seed(args: &[String]) {
    let (_, flags) = parse_flags(args, &[]);
    let dry = flags.iter().any(|(k, _)| k == "dry-run");
    let list = flags.iter().any(|(k, _)| k == "list");
    match seed::seed(dry) {
        Ok(r) => {
            if list {
                for path in &r.paths {
                    println!("{path}");
                }
            }
            let verb = if dry { "would seed" } else { "seeded" };
            eprintln!(
                "ccd: {} {} dirs from {} cd events ({} resolved){}",
                verb,
                r.unique_dirs,
                r.events,
                r.resolved,
                if dry { " [dry run]" } else { "" }
            );
        }
        Err(e) => {
            eprintln!("ccd: seed failed: {e}");
            exit(1);
        }
    }
}

fn cmd_init(args: &[String]) {
    let (pos, _) = parse_flags(args, &[]);
    match pos.first().copied() {
        Some("zsh") => print!("{}", include_str!("../shell/ccd.zsh")),
        Some("bash") => print!("{}", include_str!("../shell/ccd.bash")),
        Some("fish") => print!("{}", include_str!("../shell/ccd.fish")),
        _ => {
            eprintln!("ccd init: specify one of: zsh, bash, fish");
            exit(2);
        }
    }
}

fn cmd_forget(args: &[String]) {
    let (pos, _) = parse_flags(args, &[]);
    let Some(raw) = pos.first() else { usage(2) };
    let mut store = Store::load_locked();
    ensure_locked(&store);
    let mut removed = store.remove(raw);
    if let Ok(canon) = std::fs::canonicalize(raw) {
        removed |= store.remove(&canon.to_string_lossy());
    }
    if removed {
        if let Err(e) = store.save() {
            eprintln!("ccd: forget failed: {e}");
            exit(1);
        }
        eprintln!("ccd: forgot {raw}");
    } else {
        eprintln!("ccd: no entry for {raw}");
        exit(1);
    }
}

fn cmd_prune(args: &[String]) {
    let (_, flags) = parse_flags(args, &[]);
    let dry = flags.iter().any(|(k, _)| k == "dry-run");
    let mut store = Store::load_locked();
    ensure_locked(&store);
    let removed = store.prune_stale();
    if !dry && removed > 0 {
        if let Err(e) = store.save() {
            eprintln!("ccd: prune failed: {e}");
            exit(1);
        }
    }
    let suffix = if dry { " [dry run]" } else { "" };
    eprintln!("ccd: pruned {removed} stale dirs{suffix}");
}

fn cmd_stats(_args: &[String]) {
    let store = Store::load();
    let total = store.map.len();
    let live = store
        .map
        .keys()
        .filter(|p| Path::new(p.as_str()).is_dir())
        .count();
    println!("db\t{}", store.path().display());
    println!("dirs\t{total}");
    println!("live\t{live}");
    println!("stale\t{}", total.saturating_sub(live));
}

fn cmd_doctor(_args: &[String]) {
    let store = Store::load_locked();
    let path = store.path();
    println!(
        "command\t{}",
        std::env::args().next().unwrap_or_else(|| "ccd".into())
    );
    println!("db\t{}", path.display());
    println!("db_exists\t{}", path.exists());
    println!(
        "db_lock\t{}",
        if store.is_locked() {
            "ok"
        } else {
            "unavailable"
        }
    );
    if let Some(parent) = path.parent() {
        println!("db_dir\t{}", parent.display());
        println!("db_dir_exists\t{}", parent.exists());
    }
    println!("dirs\t{}", store.map.len());
}

fn canonical_dir_or_input(path: &str) -> String {
    std::fs::canonicalize(path)
        .ok()
        .filter(|p| p.is_dir())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

fn add_completion_dirs(store: &mut Store, cwd: &str, t: u64) {
    let Ok(rd) = std::fs::read_dir(cwd) else {
        return;
    };
    for ent in rd.flatten() {
        let Ok(ft) = ent.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let name = ent.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let Ok(canon) = std::fs::canonicalize(ent.path()) else {
            continue;
        };
        let path = canon.to_string_lossy().into_owned();
        store.map.entry(path).or_insert(Entry {
            score: 2.0,
            last: t,
        });
    }
}

fn ensure_locked(store: &Store) {
    if !store.is_locked() {
        eprintln!("ccd: database is busy; try again");
        exit(1);
    }
}
