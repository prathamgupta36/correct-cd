//! Frecency store: a flat TSV file, one line per directory:
//!     <path>\t<score>\t<last_epoch>
//! `score` is the half-life-decayed visit mass; `last` is when it was updated.
//! Decay is lazy (applied on read/update), so there's no cron/aging job.

use std::collections::HashMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct Entry {
    pub score: f64,
    pub last: u64,
}

pub struct Store {
    pub map: HashMap<String, Entry>,
    path: PathBuf,
    lock: Option<LockGuard>,
}

struct LockGuard {
    path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn db_path() -> PathBuf {
    if let Ok(p) = std::env::var("CCD_DB") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    let base = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".local/share")
        });
    base.join("ccd").join("db.tsv")
}

impl Store {
    pub fn load() -> Store {
        Store::load_from_inner(db_path(), None)
    }

    pub fn load_locked() -> Store {
        let path = db_path();
        let lock = acquire_lock(&path);
        Store::load_from_inner(path, lock)
    }

    #[cfg(test)]
    pub fn load_from(path: PathBuf) -> Store {
        Store::load_from_inner(path, None)
    }

    fn load_from_inner(path: PathBuf, lock: Option<LockGuard>) -> Store {
        let mut map = HashMap::new();
        if let Ok(s) = fs::read_to_string(&path) {
            for line in s.lines() {
                let mut it = line.split('\t');
                if let (Some(p), Some(sc), Some(ts)) = (it.next(), it.next(), it.next()) {
                    if let (Ok(score), Ok(last)) = (sc.parse::<f64>(), ts.parse::<u64>()) {
                        map.insert(p.to_string(), Entry { score, last });
                    }
                }
            }
        }
        Store { map, path, lock }
    }

    pub fn save(&self) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = tmp_path(&self.path);
        {
            let mut f = fs::File::create(&tmp)?;
            let mut rows: Vec<_> = self.map.iter().collect();
            rows.sort_by_key(|(path, _)| *path);
            for (p, e) in rows {
                writeln!(f, "{}\t{}\t{}", p, e.score, e.last)?;
            }
            f.sync_all()?;
        }
        fs::rename(&tmp, &self.path) // atomic replace
    }

    /// Record a visit of `weight` at time `t`, applying half-life decay.
    pub fn add(&mut self, path: &str, weight: f64, t: u64, h_secs: f64) {
        let e = self.map.entry(path.to_string()).or_insert(Entry {
            score: 0.0,
            last: t,
        });
        let dt = t.saturating_sub(e.last) as f64;
        e.score = e.score * 2f64.powf(-dt / h_secs) + weight;
        e.last = t;
    }

    pub fn remove(&mut self, path: &str) -> bool {
        self.map.remove(path).is_some()
    }

    pub fn prune_stale(&mut self) -> usize {
        let before = self.map.len();
        self.map.retain(|p, _| std::path::Path::new(p).is_dir());
        before - self.map.len()
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn is_locked(&self) -> bool {
        self.lock.is_some()
    }
}

fn tmp_path(path: &std::path::Path) -> PathBuf {
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    path.with_extension(format!("tmp.{pid}.{nanos}"))
}

fn acquire_lock(path: &std::path::Path) -> Option<LockGuard> {
    let lock_path = path.with_extension("lock");
    if let Some(dir) = lock_path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    for _ in 0..100 {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(mut f) => {
                let _ = writeln!(f, "{}", std::process::id());
                return Some(LockGuard { path: lock_path });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if is_stale_lock(&lock_path) {
                    let _ = fs::remove_file(&lock_path);
                    continue;
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return None,
        }
    }
    None
}

fn is_stale_lock(path: &std::path::Path) -> bool {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age > Duration::from_secs(30))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("ccd-{name}-{}-{nanos}.tsv", std::process::id()))
    }

    #[test]
    fn add_applies_half_life_decay() {
        let mut store = Store::load_from(temp_db("decay"));
        store.add("/tmp/example", 1.0, 0, 10.0);
        store.add("/tmp/example", 1.0, 10, 10.0);
        let score = store.map["/tmp/example"].score;
        assert!((score - 1.5).abs() < 0.0001);
    }

    #[test]
    fn save_and_load_round_trip_sorted_rows() {
        let path = temp_db("roundtrip");
        let mut store = Store::load_from(path.clone());
        store.add("/tmp/z", 1.0, 1, 10.0);
        store.add("/tmp/a", 2.0, 2, 10.0);
        store.save().unwrap();

        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.starts_with("/tmp/a\t"));

        let loaded = Store::load_from(path.clone());
        assert_eq!(loaded.map.len(), 2);
        assert!(loaded.map.contains_key("/tmp/z"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn prune_and_remove_update_entries() {
        let mut store = Store::load_from(temp_db("prune"));
        store.add("/definitely/missing/ccd-test", 1.0, 1, 10.0);
        assert_eq!(store.prune_stale(), 1);
        store.add("/tmp", 1.0, 1, 10.0);
        assert!(store.remove("/tmp"));
        assert!(!store.remove("/tmp"));
    }
}
