//! Fast FILETIME -> local time text (`dd/mm/yyyy HH:MM:SS`).
//!
//! The straightforward implementation (`DateTime<Utc>` -> `Local` -> `format`)
//! asks the OS for the time zone rules on every call, which is far too slow
//! when a date filter has to format ~1,000,000 rows. Here the UTC offset is
//! looked up through chrono's `Local` only once per 15-minute UTC bucket (every
//! DST change happens on such a boundary) and cached; the calendar date is then
//! computed with integer arithmetic (Howard Hinnant's `civil_from_days`).
//!
//! The output is identical to the chrono version for every input:
//!
//! ```ignore
//! fn reference(ft: i64) -> String {
//!     use chrono::{DateTime, Local, Utc};
//!     let secs = ft.div_euclid(10_000_000) - 11_644_473_600;
//!     let nanos = (ft.rem_euclid(10_000_000) * 100) as u32;
//!     match DateTime::<Utc>::from_timestamp(secs, nanos) {
//!         Some(utc) => utc.with_timezone(&Local).format("%d/%m/%Y %H:%M:%S").to_string(),
//!         None => String::from("?"),
//!     }
//! }
//! ```

use chrono::{DateTime, Local, Utc};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write as _;

/// Seconds between 1601-01-01 (FILETIME epoch) and 1970-01-01 (Unix epoch).
const FILETIME_TO_UNIX_SECS: i64 = 11_644_473_600;
/// Offsets are cached per bucket of this many UTC seconds.
const BUCKET_SECS: i64 = 15 * 60;
/// Upper bound on cached buckets (bounds memory on garbage input).
const MAX_CACHED_BUCKETS: usize = 8192;

struct OffsetCache {
    /// Most recently used `(bucket, offset)`; rows are usually near each other.
    last: Option<(i64, i32)>,
    map: HashMap<i64, i32>,
}

thread_local! {
    static CACHE: RefCell<OffsetCache> = RefCell::new(OffsetCache {
        last: None,
        map: HashMap::new(),
    });
}

/// What chrono says the local UTC offset (seconds east) is at `secs` (Unix
/// time), or `None` when chrono cannot represent that instant.
fn chrono_offset(secs: i64) -> Option<i32> {
    DateTime::<Utc>::from_timestamp(secs, 0).map(|utc| utc.with_timezone(&Local).offset().local_minus_utc())
}

/// Cached local UTC offset in seconds for the Unix timestamp `secs`.
fn local_offset(secs: i64) -> Option<i32> {
    let bucket = secs.div_euclid(BUCKET_SECS);
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((b, off)) = cache.last {
            if b == bucket {
                return Some(off);
            }
        }
        if let Some(&off) = cache.map.get(&bucket) {
            cache.last = Some((bucket, off));
            return Some(off);
        }
        let off = chrono_offset(secs)?;
        if cache.map.len() >= MAX_CACHED_BUCKETS {
            cache.map.clear();
        }
        cache.map.insert(bucket, off);
        cache.last = Some((bucket, off));
        Some(off)
    })
}

/// Days since 1970-01-01 -> proleptic Gregorian `(year, month 1..=12, day 1..=31)`.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[inline]
fn push2(out: &mut String, v: u32) {
    debug_assert!(v < 100);
    out.push((b'0' + (v / 10) as u8) as char);
    out.push((b'0' + (v % 10) as u8) as char);
}

/// Mirrors chrono's `%Y`: at least four digits, and a sign when outside 0..=9999.
fn push_year(out: &mut String, y: i64) {
    if (0..=9999).contains(&y) {
        let y = y as u32;
        push2(out, y / 100);
        push2(out, y % 100);
    } else {
        // Rare (garbage timestamps only), so the slow generic path is fine.
        let _ = if y < 0 { write!(out, "-{:04}", y.unsigned_abs()) } else { write!(out, "+{y}") };
    }
}

/// Appends the local time text for `ft` (a FILETIME) to `out`, or `?` when the
/// value is outside the range chrono supports. Does not allocate when `out`
/// already has capacity (a few dozen bytes).
pub fn format_filetime_into(ft: i64, out: &mut String) {
    let secs = ft.div_euclid(10_000_000) - FILETIME_TO_UNIX_SECS;
    let Some(offset) = local_offset(secs) else {
        out.push('?');
        return;
    };
    let local = secs + i64::from(offset);
    let days = local.div_euclid(86_400);
    let sod = local.rem_euclid(86_400) as u32; // seconds of day
    let (y, m, d) = civil_from_days(days);

    push2(out, d);
    out.push('/');
    push2(out, m);
    out.push('/');
    push_year(out, y);
    out.push(' ');
    push2(out, sod / 3600);
    out.push(':');
    push2(out, sod / 60 % 60);
    out.push(':');
    push2(out, sod % 60);
}

/// Same output as the chrono version in the module docs, byte for byte, for every input.
pub fn format_filetime(ft: i64) -> String {
    let mut s = String::with_capacity(24);
    format_filetime_into(ft, &mut s);
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::hint::black_box;
    use std::time::Instant;

    /// The original implementation, kept as the oracle.
    fn reference(ft: i64) -> String {
        let secs = ft.div_euclid(10_000_000) - 11_644_473_600;
        let nanos = (ft.rem_euclid(10_000_000) * 100) as u32;
        match DateTime::<Utc>::from_timestamp(secs, nanos) {
            Some(utc) => utc.with_timezone(&Local).format("%d/%m/%Y %H:%M:%S").to_string(),
            None => String::from("?"),
        }
    }

    /// FILETIME for a Unix timestamp.
    fn ft_of(unix_secs: i64) -> i64 {
        (unix_secs + FILETIME_TO_UNIX_SECS) * 10_000_000
    }

    struct XorShift(u64);
    impl XorShift {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
    }

    #[test]
    fn random_1601_to_2100_matches_chrono() {
        // 1601-01-01 .. 2100-01-01 in 100ns units.
        let span: u64 = ((2100 - 1601) as f64 * 365.2425 * 86_400.0 * 1e7) as u64;
        let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
        for _ in 0..300_000 {
            let ft = (rng.next() % span) as i64;
            assert_eq!(format_filetime(ft), reference(ft), "ft={ft}");
        }
    }

    #[test]
    fn every_15_minutes_of_2024_and_2025_matches_chrono() {
        let start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
        let end = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
        let mut t = start;
        let mut n = 0;
        while t < end {
            // Exactly on the boundary and one second either side.
            for dt in [-1, 0, 1, 899] {
                let ft = ft_of(t + dt) + 1234567; // plus a sub-second part
                assert_eq!(format_filetime(ft), reference(ft), "unix={}", t + dt);
            }
            t += 900;
            n += 1;
        }
        assert_eq!(n, 2 * 365 * 96 + 96); // 2024 is a leap year
    }

    #[test]
    fn edge_values_match_chrono() {
        let mut vals = vec![0, 1, -1, 9_999_999, 10_000_000, -10_000_000, i64::MAX, i64::MIN, i64::MAX - 1, i64::MIN + 1];
        vals.push(ft_of(0));
        vals.push(ft_of(-1));
        vals.push(ft_of(86_399));
        vals.push(ft_of(951_782_400)); // 2000-02-29
        vals.push(ft_of(4_102_444_800)); // 2100-01-01
        vals.push(ft_of(253_402_300_799)); // 9999-12-31 23:59:59 UTC
        vals.push(ft_of(253_402_300_800)); // year 10000
        vals.push(ft_of(-62_167_219_200)); // 0000-01-01
        vals.push(ft_of(-62_167_219_201)); // year -1
        for ft in vals {
            assert_eq!(format_filetime(ft), reference(ft), "ft={ft}");
        }
    }

    #[test]
    fn full_i64_range_matches_chrono() {
        let mut rng = XorShift(0xDEAD_BEEF_CAFE_F00D);
        for _ in 0..100_000 {
            let ft = rng.next() as i64;
            assert_eq!(format_filetime(ft), reference(ft), "ft={ft}");
        }
        // Small values around the epoch (negative and positive).
        for ft in (-5_000_000_000i64..5_000_000_000).step_by(37_000_001) {
            assert_eq!(format_filetime(ft), reference(ft), "ft={ft}");
        }
    }

    #[test]
    fn civil_from_days_matches_chrono_naive_date() {
        // Every day from year -1000 to 3000.
        let first = NaiveDate::from_ymd_opt(-1000, 1, 1).unwrap().signed_duration_since(NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()).num_days();
        let last = NaiveDate::from_ymd_opt(3000, 12, 31).unwrap().signed_duration_since(NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()).num_days();
        use chrono::Datelike;
        let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
        for days in first..=last {
            let (y, m, d) = civil_from_days(days);
            let want = epoch + chrono::Duration::days(days);
            assert_eq!((y, m, d), (i64::from(want.year()), want.month(), want.day()), "days={days}");
        }
    }

    #[test]
    fn into_appends_and_does_not_clobber() {
        let mut s = String::from("x:");
        format_filetime_into(ft_of(1_700_000_000), &mut s);
        assert_eq!(s, format!("x:{}", reference(ft_of(1_700_000_000))));
        assert_eq!(s.len(), 2 + 19);
    }

    #[test]
    fn into_does_not_reallocate_with_capacity() {
        let mut s = String::with_capacity(32);
        let ptr = s.as_ptr();
        format_filetime_into(ft_of(1_700_000_000), &mut s);
        assert_eq!(s.as_ptr(), ptr);
        assert_eq!(s.capacity(), 32);
    }

    #[test]
    fn cache_survives_many_buckets() {
        // More distinct buckets than MAX_CACHED_BUCKETS forces the cache to reset.
        let base = ft_of(1_600_000_000);
        for i in 0..(MAX_CACHED_BUCKETS as i64 * 2 + 10) {
            let ft = base + i * 900 * 10_000_000;
            assert_eq!(format_filetime(ft), reference(ft));
        }
    }

    #[test]
    fn speed_one_million_calls() {
        // Timestamps over two years, roughly increasing like a sorted journal, with some jitter.
        let base = ft_of(1_700_000_000);
        let fts: Vec<i64> = (0..1_000_000i64).map(|i| base + i * 60 * 10_000_000 + (i * 7919 % 1000) * 10_000).collect();

        let t = Instant::now();
        let mut total = 0usize;
        let mut buf = String::with_capacity(32);
        for &ft in &fts {
            buf.clear();
            format_filetime_into(black_box(ft), &mut buf);
            total += buf.len();
        }
        let fast = t.elapsed();

        let t = Instant::now();
        let mut total2 = 0usize;
        for &ft in &fts {
            total2 += reference(black_box(ft)).len();
        }
        let slow = t.elapsed();
        assert_eq!(total, total2);
        eprintln!("1,000,000 format_filetime_into calls: new = {fast:?}, chrono reference = {slow:?}");

        let t = Instant::now();
        let mut total3 = 0usize;
        for &ft in &fts {
            total3 += format_filetime(black_box(ft)).len();
        }
        eprintln!("1,000,000 format_filetime (allocating) calls: new = {:?}", t.elapsed());
        assert_eq!(total, total3);
    }
}
