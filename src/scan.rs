//! Background scan: open volume -> query journal -> read entries -> resolve
//! directories -> build indexes. Phases match the C# status messages
//! (`statustitle1..6`, negative on failure).

use crate::model::{DirInfo, Entry, JournalInfo, ScanResult, VolumeInfo};
use crate::ntfs::{Volume, Win32Error};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};

pub struct ScanOutput {
    pub result: ScanResult,
    /// Same order as `result.volumes`; kept open so the entry window can
    /// resolve a file's current path. Empty for demo data.
    pub volumes: Vec<Arc<Volume>>,
}

#[derive(Default)]
pub struct ScanProgress {
    /// 1..=6 while running / done, -1..=-5 on failure (same as C#).
    pub phase: i32,
    /// Progress inside the current phase, 0..1.
    pub fraction: f32,
    pub error: Option<Win32Error>,
    /// Drive being scanned and its position, for "C:\ (1/3)".
    pub drive: Option<char>,
    pub drive_pos: usize,
    pub drive_count: usize,
    /// Drives skipped in a multi-drive scan: letter and the error.
    pub skipped: Vec<(char, Win32Error)>,
    pub output: Option<ScanOutput>,
}

pub type SharedProgress = Arc<Mutex<ScanProgress>>;

/// Scans `letters` one after another into a single result. With one drive
/// a failure stops the scan like the C# version; with several, a failing
/// drive is skipped and reported.
pub fn start(letters: Vec<char>, repaint: impl Fn() + Send + Sync + 'static) -> SharedProgress {
    let shared: SharedProgress = Arc::new(Mutex::new(ScanProgress {
        phase: 1,
        drive_count: letters.len(),
        ..Default::default()
    }));
    let state = shared.clone();
    std::thread::Builder::new()
        .name("journal-scan".into())
        .spawn(move || {
            let set = |phase: i32, fraction: f32| {
                let mut p = state.lock().unwrap();
                p.phase = phase;
                p.fraction = fraction;
                drop(p);
                repaint();
            };
            let fail = |phase: i32, err: Win32Error| {
                let mut p = state.lock().unwrap();
                p.phase = -phase;
                p.error = Some(err);
                drop(p);
                repaint();
            };

            let single = letters.len() == 1;
            let mut parts = Vec::new();
            let mut handles = Vec::new();
            let mut last_err = None;
            for (pos, &letter) in letters.iter().enumerate() {
                {
                    let mut p = state.lock().unwrap();
                    p.drive = Some(letter);
                    p.drive_pos = pos + 1;
                }
                match scan_one(letter, &set) {
                    Ok((volume, part)) => {
                        handles.push(volume);
                        parts.push(part);
                    }
                    Err((phase, e)) if single => return fail(phase, e),
                    Err((phase, e)) => {
                        state.lock().unwrap().skipped.push((letter, e));
                        last_err = Some((phase, e));
                    }
                }
            }
            if parts.is_empty() {
                let (phase, e) = last_err.unwrap_or((1, Win32Error(0)));
                return fail(phase, e);
            }

            set(5, 0.0);
            let result = build_result(parts);

            let mut p = state.lock().unwrap();
            p.output = Some(ScanOutput { result, volumes: handles });
            p.phase = 6;
            p.fraction = 1.0;
            drop(p);
            repaint();
        })
        .expect("failed to spawn scan thread");
    shared
}

pub type Part = (VolumeInfo, Vec<Entry>, HashMap<u64, DirInfo>);

/// Phases 1-4 for one drive. Errors carry the phase they happened in.
fn scan_one(letter: char, set: &(impl Fn(i32, f32) + Sync)) -> Result<(Arc<Volume>, Part), (i32, Win32Error)> {
    set(1, 0.0);
    let volume = Arc::new(Volume::open(letter).map_err(|e| (1, e))?);

    set(2, 0.0);
    let journal = volume.query_journal().map_err(|e| (2, e))?;

    set(3, 0.0);
    let mut last = 0.0f32;
    let entries = volume
        .read_entries(&journal, |f| {
            if f - last >= 0.01 {
                last = f;
                set(3, f);
            }
        })
        .map_err(|e| (3, e))?;

    set(4, 0.0);
    let dirs = resolve_dirs(&volume, letter, &entries, |f| set(4, f));
    Ok((volume, (VolumeInfo { letter, journal }, entries, dirs)))
}

/// Resolves every parent reference to a path, in parallel. Directories that
/// no longer exist are rebuilt from the journal's own names when possible.
fn resolve_dirs(
    volume: &Volume,
    letter: char,
    entries: &[Entry],
    progress: impl Fn(f32) + Sync,
) -> HashMap<u64, DirInfo> {
    let ids: Vec<u64> = entries
        .iter()
        .map(|e| e.parent_ref)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 16);
    let chunk = ids.len().div_ceil(threads).max(1);
    let done = AtomicUsize::new(0);
    let total = ids.len().max(1);

    let resolved: Vec<(u64, Option<String>)> = std::thread::scope(|s| {
        let handles: Vec<_> = ids
            .chunks(chunk)
            .map(|part| {
                let done = &done;
                let progress = &progress;
                s.spawn(move || {
                    let mut out = Vec::with_capacity(part.len());
                    for &id in part {
                        out.push((id, volume.resolve_path(id)));
                        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                        if n.is_multiple_of(256) {
                            progress(n as f32 / total as f32);
                        }
                    }
                    out
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });

    let known: HashMap<u64, String> =
        resolved.iter().filter_map(|(id, p)| p.clone().map(|p| (*id, p))).collect();

    // Last known name and parent of every reference seen in the journal.
    let mut names: HashMap<u64, (&str, u64)> = HashMap::new();
    for e in entries {
        names.insert(e.file_ref, (e.name.as_str(), e.parent_ref));
    }

    resolved
        .into_iter()
        .map(|(id, path)| {
            let info = match path {
                Some(path) => DirInfo { path, exists: true },
                None => DirInfo { path: rebuild_path(id, letter, &known, &names), exists: false },
            };
            (id, info)
        })
        .collect()
}

fn rebuild_path(
    id: u64,
    letter: char,
    known: &HashMap<u64, String>,
    names: &HashMap<u64, (&str, u64)>,
) -> String {
    let mut parts: Vec<&str> = Vec::new();
    let mut cur = id;
    for _ in 0..256 {
        if let Some(base) = known.get(&cur) {
            return join(base, &parts);
        }
        match names.get(&cur) {
            Some(&(name, parent)) if parent != cur => {
                parts.push(name);
                cur = parent;
            }
            _ => break,
        }
    }
    // Chain broken: keep the unknown reference, under its drive so several
    // drives don't mix in the tree.
    join(&format!("{letter}:\\[{cur}]"), &parts)
}

fn join(base: &str, rev_parts: &[&str]) -> String {
    let mut s = base.trim_end_matches('\\').to_string();
    for p in rev_parts.iter().rev() {
        s.push('\\');
        s.push_str(p);
    }
    if s.ends_with(':') {
        s.push('\\');
    }
    s
}

pub fn build_result(parts: Vec<Part>) -> ScanResult {
    let mut result = ScanResult::default();
    for (vol, (info, mut entries, dirs)) in parts.into_iter().enumerate() {
        let vol = vol as u8;
        entries.sort_by_key(|e| e.usn);
        for mut e in entries {
            e.vol = vol;
            let i = result.entries.len() as u32;
            result.by_file.entry(e.file_key()).or_default().push(i);
            result.by_dir.entry(e.dir_key()).or_default().push(i);
            result.entries.push(e);
        }
        result.dirs.extend(dirs.into_iter().map(|(id, d)| ((vol, id), d)));
        result.volumes.push(info);
    }
    result
}

/// New records found by live monitoring.
pub struct LiveBatch {
    pub vol: u8,
    pub entries: Vec<Entry>,
    pub dirs: Vec<(u64, DirInfo)>,
    pub journal: JournalInfo,
}

pub enum LiveMsg {
    Batch(LiveBatch),
    Stopped(Win32Error),
}

/// Running live monitor. Dropping it stops the thread.
pub struct Live {
    stop: Arc<AtomicBool>,
    pub rx: mpsc::Receiver<LiveMsg>,
}

impl Drop for Live {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Polls the journal once a second and sends whatever was written since
/// `journal.next_usn`. `known_dirs` are the parent references already
/// resolved, so only new directories cost an OpenFileById.
pub fn start_live(
    vol: u8,
    volume: Arc<Volume>,
    journal: JournalInfo,
    mut known_dirs: HashSet<u64>,
    repaint: impl Fn() + Send + 'static,
) -> Live {
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let stop_flag = stop.clone();
    std::thread::Builder::new()
        .name("journal-live".into())
        .spawn(move || {
            let mut next = journal.next_usn;
            loop {
                for _ in 0..10 {
                    if stop_flag.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                let info = match volume.query_journal() {
                    Ok(i) if i.journal_id == journal.journal_id => i,
                    // A different ID means the journal was deleted and recreated.
                    Ok(_) => {
                        let _ = tx.send(LiveMsg::Stopped(Win32Error(crate::ntfs::ERROR_JOURNAL_NOT_ACTIVE)));
                        repaint();
                        return;
                    }
                    Err(e) => {
                        let _ = tx.send(LiveMsg::Stopped(e));
                        repaint();
                        return;
                    }
                };
                if info.next_usn <= next {
                    continue;
                }
                let entries =
                    match volume.read_range(info.journal_id, next, info.next_usn, info.lowest_valid_usn, |_| {}) {
                        Ok(e) => e,
                        Err(e) => {
                            let _ = tx.send(LiveMsg::Stopped(e));
                            repaint();
                            return;
                        }
                    };
                next = info.next_usn;
                if entries.is_empty() {
                    continue;
                }
                let dirs = entries
                    .iter()
                    .filter(|e| known_dirs.insert(e.parent_ref))
                    .map(|e| {
                        let info = match volume.resolve_path(e.parent_ref) {
                            Some(path) => DirInfo { path, exists: true },
                            None => DirInfo { path: e.parent_ref.to_string(), exists: false },
                        };
                        (e.parent_ref, info)
                    })
                    .collect();
                if tx.send(LiveMsg::Batch(LiveBatch { vol, entries, dirs, journal: info })).is_err() {
                    return;
                }
                repaint();
            }
        })
        .expect("failed to spawn live thread");
    Live { stop, rx }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuilds_deleted_chain() {
        let mut known = HashMap::new();
        known.insert(5u64, "C:\\".to_string());
        known.insert(7u64, "C:\\Users".to_string());
        let mut names = HashMap::new();
        names.insert(100u64, ("old", 7u64));
        names.insert(101u64, ("inner", 100u64));
        assert_eq!(rebuild_path(101, 'C', &known, &names), "C:\\Users\\old\\inner");
        names.insert(200u64, ("top", 5u64));
        assert_eq!(rebuild_path(200, 'C', &known, &names), "C:\\top");
        names.insert(300u64, ("lost", 999u64));
        assert_eq!(rebuild_path(300, 'D', &known, &names), "D:\\[999]\\lost");
        assert_eq!(rebuild_path(42, 'C', &known, &names), "C:\\[42]");
    }
}
