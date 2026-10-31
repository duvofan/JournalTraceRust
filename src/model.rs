//! Parsed journal data shared by the scanner and the UI.

use std::collections::HashMap;

/// Every USN reason bit we know about, in the order `i18n::reason_names` uses.
pub const REASON_FLAGS: [u32; 24] = [
    0x0000_0001, // DATA_OVERWRITE
    0x0000_0002, // DATA_EXTEND
    0x0000_0004, // DATA_TRUNCATION
    0x0000_0010, // NAMED_DATA_OVERWRITE
    0x0000_0020, // NAMED_DATA_EXTEND
    0x0000_0040, // NAMED_DATA_TRUNCATION
    0x0000_0100, // FILE_CREATE
    0x0000_0200, // FILE_DELETE
    0x0000_0400, // EA_CHANGE
    0x0000_0800, // SECURITY_CHANGE
    0x0000_1000, // RENAME_OLD_NAME
    0x0000_2000, // RENAME_NEW_NAME
    0x0000_4000, // INDEXABLE_CHANGE
    0x0000_8000, // BASIC_INFO_CHANGE
    0x0001_0000, // HARD_LINK_CHANGE
    0x0002_0000, // COMPRESSION_CHANGE
    0x0004_0000, // ENCRYPTION_CHANGE
    0x0008_0000, // OBJECT_ID_CHANGE
    0x0010_0000, // REPARSE_POINT_CHANGE
    0x0020_0000, // STREAM_CHANGE
    0x0040_0000, // TRANSACTED_CHANGE
    0x0080_0000, // INTEGRITY_CHANGE
    0x0100_0000, // DESIRED_STORAGE_CLASS_CHANGE
    0x8000_0000, // CLOSE
];

pub const REASON_FILE_CREATE: u32 = 0x0000_0100;
pub const REASON_FILE_DELETE: u32 = 0x0000_0200;
pub const REASON_RENAME_OLD_NAME: u32 = 0x0000_1000;
pub const REASON_RENAME_NEW_NAME: u32 = 0x0000_2000;
pub const REASON_BASIC_INFO_CHANGE: u32 = 0x0000_8000;
pub const REASON_STREAM_CHANGE: u32 = 0x0020_0000;

/// One record of the journal.
#[derive(Clone, Debug)]
pub struct Entry {
    pub usn: i64,
    pub name: String,
    pub file_ref: u64,
    pub parent_ref: u64,
    /// FILETIME: 100ns ticks since 1601-01-01 UTC.
    pub timestamp: i64,
    pub reason: u32,
    pub attributes: u32,
    /// Index into `ScanResult::volumes`.
    pub vol: u8,
}

/// A file reference is only unique inside its volume.
pub type Ref = (u8, u64);

impl Entry {
    pub fn file_key(&self) -> Ref {
        (self.vol, self.file_ref)
    }
    pub fn dir_key(&self) -> Ref {
        (self.vol, self.parent_ref)
    }
}

/// A directory referenced as a parent by at least one entry.
#[derive(Clone, Debug)]
pub struct DirInfo {
    /// Full path. For directories that no longer exist this is rebuilt from
    /// the journal's own rename/create records when possible, otherwise the
    /// numeric file reference under the drive root.
    pub path: String,
    /// The directory still exists on the volume (OpenFileById succeeded).
    pub exists: bool,
}

#[derive(Clone, Debug, Default)]
pub struct JournalInfo {
    pub journal_id: u64,
    pub first_usn: i64,
    pub next_usn: i64,
    pub lowest_valid_usn: i64,
    pub max_usn: i64,
    pub maximum_size: u64,
    pub allocation_delta: u64,
}

#[derive(Clone, Debug)]
pub struct VolumeInfo {
    pub letter: char,
    pub journal: JournalInfo,
}

/// Everything a scan produced, for one or more volumes. Entries are grouped
/// by volume and sorted by USN inside each.
#[derive(Default)]
pub struct ScanResult {
    pub volumes: Vec<VolumeInfo>,
    pub entries: Vec<Entry>,
    pub dirs: HashMap<Ref, DirInfo>,
    /// file -> indices into `entries`
    pub by_file: HashMap<Ref, Vec<u32>>,
    /// parent directory -> indices into `entries`
    pub by_dir: HashMap<Ref, Vec<u32>>,
}

impl ScanResult {
    pub fn dir_path(&self, e: &Entry) -> &str {
        self.dirs.get(&e.dir_key()).map(|d| d.path.as_str()).unwrap_or("")
    }

    pub fn dir_exists(&self, e: &Entry) -> bool {
        self.dirs.get(&e.dir_key()).is_some_and(|d| d.exists)
    }

    pub fn drive(&self, e: &Entry) -> char {
        self.volumes.get(e.vol as usize).map_or('?', |v| v.letter)
    }

    /// Every record of the same file, in USN order.
    pub fn file_history(&self, e: &Entry) -> &[u32] {
        self.by_file.get(&e.file_key()).map_or(&[], Vec::as_slice)
    }

    pub fn oldest_timestamp(&self) -> Option<i64> {
        self.entries.iter().map(|e| e.timestamp).min()
    }

    pub fn file_count(&self) -> usize {
        self.by_file.len()
    }

    /// Adds entries read by live monitoring and returns their index range.
    pub fn append(
        &mut self,
        vol: u8,
        entries: Vec<Entry>,
        dirs: Vec<(u64, DirInfo)>,
        journal: JournalInfo,
    ) -> std::ops::Range<u32> {
        let start = self.entries.len() as u32;
        for mut e in entries {
            e.vol = vol;
            let i = self.entries.len() as u32;
            self.by_file.entry(e.file_key()).or_default().push(i);
            self.by_dir.entry(e.dir_key()).or_default().push(i);
            self.entries.push(e);
        }
        self.dirs.extend(dirs.into_iter().map(|(id, d)| ((vol, id), d)));
        if let Some(v) = self.volumes.get_mut(vol as usize) {
            v.journal = journal;
        }
        start..self.entries.len() as u32
    }
}

/// What happened to one file over its whole history in the journal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileStory {
    /// Every distinct name, in the order it was used (rename chain).
    pub names: Vec<String>,
    pub created: Option<i64>,
    pub deleted: Option<i64>,
}

impl FileStory {
    pub fn of(entries: &[Entry], history: &[u32]) -> FileStory {
        let mut story = FileStory::default();
        for &i in history {
            let e = &entries[i as usize];
            if story.names.last() != Some(&e.name) {
                story.names.push(e.name.clone());
            }
            if e.reason & REASON_FILE_CREATE != 0 && story.created.is_none() {
                story.created = Some(e.timestamp);
            }
            if e.reason & REASON_FILE_DELETE != 0 {
                story.deleted = Some(e.timestamp);
            }
        }
        story
    }

    /// FILETIME ticks between creation and deletion, when both are in the journal.
    pub fn lifetime(&self) -> Option<i64> {
        match (self.created, self.deleted) {
            (Some(c), Some(d)) if d >= c => Some(d - c),
            _ => None,
        }
    }
}

/// Short human text for a duration in FILETIME ticks: "850 ms", "5 s", "3 min 12 s", "2 h 5 min", "3 d 4 h".
pub fn format_duration(ticks: i64) -> String {
    let ms = ticks / 10_000;
    if ms < 1000 {
        return format!("{ms} ms");
    }
    let s = ms / 1000;
    match s {
        0..=59 => format!("{s} s"),
        60..=3599 => format!("{} min {} s", s / 60, s % 60),
        3600..=86_399 => format!("{} h {} min", s / 3600, s % 3600 / 60),
        _ => format!("{} d {} h", s / 86_400, s % 86_400 / 3600),
    }
}

/// Converts a FILETIME to local time text, same format as the C# version.
pub fn format_filetime(ft: i64) -> String {
    crate::timefmt::format_filetime(ft)
}

/// Builds the " | " separated reason text for a reason mask.
pub fn reason_text(mask: u32, names: &[&str; 24]) -> String {
    let mut out = String::new();
    for (flag, name) in REASON_FLAGS.iter().zip(names.iter()) {
        if mask & flag != 0 {
            if !out.is_empty() {
                out.push_str(" | ");
            }
            out.push_str(name);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, ts: i64, reason: u32) -> Entry {
        Entry { usn: ts, name: name.into(), file_ref: 1, parent_ref: 2, timestamp: ts, reason, attributes: 0, vol: 0 }
    }

    #[test]
    fn story_tracks_renames_and_lifetime() {
        let entries = vec![
            e("a.exe", 100, REASON_FILE_CREATE),
            e("a.exe", 110, 0x2),
            e("a.exe", 120, REASON_RENAME_OLD_NAME),
            e("b.tmp", 120, REASON_RENAME_NEW_NAME),
            e("b.tmp", 50_000_100, REASON_FILE_DELETE),
        ];
        let story = FileStory::of(&entries, &[0, 1, 2, 3, 4]);
        assert_eq!(story.names, vec!["a.exe", "b.tmp"]);
        assert_eq!(story.lifetime(), Some(50_000_000));
        assert_eq!(format_duration(50_000_000), "5 s");
    }

    #[test]
    fn durations() {
        assert_eq!(format_duration(8_500_000), "850 ms");
        assert_eq!(format_duration(192 * 10_000_000), "3 min 12 s");
        assert_eq!(format_duration((2 * 3600 + 300) * 10_000_000), "2 h 5 min");
        assert_eq!(format_duration((3 * 86_400 + 4 * 3600) * 10_000_000), "3 d 4 h");
    }
}
