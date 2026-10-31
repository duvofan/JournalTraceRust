//! Generated data for `cargo run --example demo`: lets the UI be tried
//! without administrator rights or a real journal. Two drives, so the drive
//! column, the "all drives" view and the timeline have something to show.

use crate::model::{DirInfo, Entry, JournalInfo, VolumeInfo, REASON_FLAGS};
use crate::scan::{build_result, Part, ScanOutput};
use std::collections::HashMap;

pub(crate) fn generate(count: usize) -> ScanOutput {
    let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
    let mut rand = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    let drives: [(char, &[&str]); 2] = [
        (
            'C',
            &[
                "C:\\",
                "C:\\Windows",
                "C:\\Windows\\Prefetch",
                "C:\\Windows\\System32",
                "C:\\Windows\\Temp",
                "C:\\Users\\demo\\AppData\\Local\\Temp",
                "C:\\Users\\demo\\Downloads",
                "C:\\Users\\demo\\Desktop",
                "C:\\Program Files\\Oyun",
                "C:\\Users\\demo\\Desktop\\eski klasör",
            ],
        ),
        ('D', &["D:\\", "D:\\Oyunlar", "D:\\Yedek", "D:\\Oyunlar\\mods"]),
    ];
    let names = [
        "svchost.exe", "RUNDLL32.EXE-1A2B3C4D.pf", "cheat.dll", "notlar.txt", "rapor_ğüşiöç.docx",
        "setup.exe", "thumbcache_256.db", "O'Brien [1] 100%.log", "chrome.exe", "inject.exe",
    ];
    let combos = [
        0x8000_0100u32,
        0x0000_0002,
        0x8000_0200,
        0x0000_1000,
        0x0000_2000,
        0x8000_8000,
        0x0020_0000,
        0x8000_0003,
        0x0040_0000,
        REASON_FLAGS.iter().take(3).fold(0, |a, b| a | b),
    ];

    let base_time: i64 = 134_000_000_000_000_000; // 2025
    let mut parts: Vec<Part> = Vec::new();
    for (v, (letter, paths)) in drives.iter().enumerate() {
        let mut dirs = HashMap::new();
        for (i, path) in paths.iter().enumerate() {
            // The last C: directory plays one that was deleted and rebuilt from the journal.
            let exists = !(v == 0 && i + 1 == paths.len());
            dirs.insert(1000 + i as u64, DirInfo { path: path.to_string(), exists });
        }
        let dir_ids: Vec<u64> = dirs.keys().copied().collect();
        let n = if v == 0 { count } else { count / 4 };
        let mut entries = Vec::with_capacity(n + 4);
        for i in 0..n {
            let r = rand();
            // Busier in some hours than others, so the timeline has a shape.
            let jitter = ((r >> 40) % 3_000_000) as i64 * if (i / 5000) % 7 == 3 { 1 } else { 6 };
            entries.push(Entry {
                usn: 10_000 + i as i64 * 96,
                name: names[(r % names.len() as u64) as usize].to_string(),
                file_ref: 50_000 + (r >> 8) % 20_000,
                parent_ref: dir_ids[((r >> 24) % dir_ids.len() as u64) as usize],
                timestamp: base_time + i as i64 * 3_000_000 + jitter,
                reason: combos[((r >> 32) % combos.len() as u64) as usize],
                attributes: 0x20,
                vol: 0,
            });
        }
        if v == 0 {
            // One file with a rename chain that lived for 4 seconds.
            let story = [
                ("payload.exe", 0x0000_0100),
                ("payload.exe", 0x0000_1000),
                ("svc_update.tmp", 0x0000_2000),
                ("svc_update.tmp", 0x8000_0200),
            ];
            let t0 = base_time + 1_000_000_000;
            for (k, (name, reason)) in story.iter().enumerate() {
                entries.push(Entry {
                    usn: 10_000 + (n + k) as i64 * 96,
                    name: name.to_string(),
                    file_ref: 99_999,
                    parent_ref: 1005,
                    timestamp: t0 + k as i64 * 13_000_000,
                    reason: *reason,
                    attributes: 0x20,
                    vol: 0,
                });
            }
        }
        let journal = JournalInfo {
            journal_id: 0x01DA_0000_1234_5678 + v as u64,
            first_usn: 10_000,
            next_usn: 10_000 + entries.len() as i64 * 96,
            maximum_size: 32 * 1024 * 1024,
            ..Default::default()
        };
        parts.push((VolumeInfo { letter: *letter, journal }, entries, dirs));
    }
    ScanOutput { result: build_result(parts), volumes: Vec::new() }
}
