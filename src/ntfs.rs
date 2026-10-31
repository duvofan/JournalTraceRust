//! Thin Win32 layer: drive listing, volume handles, USN journal reads and
//! file reference -> path resolution.
//!
//! Records are parsed from the raw output buffer with bounds checks instead of
//! pointer arithmetic, so this works the same on 32 and 64 bit.

use crate::model::{Entry, JournalInfo};
use std::ffi::c_void;
use std::fmt;

type Handle = *mut c_void;

const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
const GENERIC_READ: u32 = 0x8000_0000;
const FILE_READ_ATTRIBUTES: u32 = 0x80;
const FILE_SHARE_ALL: u32 = 0x1 | 0x2 | 0x4;
const OPEN_EXISTING: u32 = 3;
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

const FSCTL_QUERY_USN_JOURNAL: u32 = 0x0009_00f4;
const FSCTL_READ_USN_JOURNAL: u32 = 0x0009_00bb;

pub const ERROR_ACCESS_DENIED: u32 = 5;
pub const ERROR_HANDLE_EOF: u32 = 38;
pub const ERROR_JOURNAL_DELETE_IN_PROGRESS: u32 = 1178;
pub const ERROR_JOURNAL_NOT_ACTIVE: u32 = 1179;
pub const ERROR_JOURNAL_ENTRY_DELETED: u32 = 1181;

#[link(name = "kernel32")]
extern "system" {
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *mut c_void,
        disposition: u32,
        flags: u32,
        template: Handle,
    ) -> Handle;
    fn CloseHandle(handle: Handle) -> i32;
    fn DeviceIoControl(
        handle: Handle,
        code: u32,
        in_buf: *const c_void,
        in_size: u32,
        out_buf: *mut c_void,
        out_size: u32,
        returned: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn OpenFileById(
        hint: Handle,
        id: *const FileIdDescriptor,
        access: u32,
        share: u32,
        security: *mut c_void,
        flags: u32,
    ) -> Handle;
    fn GetFinalPathNameByHandleW(handle: Handle, buf: *mut u16, len: u32, flags: u32) -> u32;
    fn GetLastError() -> u32;
    fn GetLogicalDrives() -> u32;
    fn GetDriveTypeW(root: *const u16) -> u32;
    fn GetVolumeInformationW(
        root: *const u16,
        name: *mut u16,
        name_len: u32,
        serial: *mut u32,
        max_component: *mut u32,
        flags: *mut u32,
        fs_name: *mut u16,
        fs_name_len: u32,
    ) -> i32;
    fn GetDiskFreeSpaceExW(dir: *const u16, avail: *mut u64, total: *mut u64, free: *mut u64) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetUserDefaultUILanguage() -> u16;
}

/// LANGID of the Windows display language.
pub fn ui_language() -> u16 {
    unsafe { GetUserDefaultUILanguage() }
}

#[link(name = "shell32")]
extern "system" {
    fn IsUserAnAdmin() -> i32;
}

#[repr(C)]
struct FileIdDescriptor {
    size: u32,
    kind: u32, // FileIdType = 0
    file_id: u64,
    _pad: u64, // the union is 16 bytes (GUID / FILE_ID_128)
}

#[repr(C)]
#[derive(Default)]
struct UsnJournalDataV0 {
    journal_id: u64,
    first_usn: i64,
    next_usn: i64,
    lowest_valid_usn: i64,
    max_usn: i64,
    maximum_size: u64,
    allocation_delta: u64,
}

#[repr(C)]
struct ReadUsnJournalDataV0 {
    start_usn: i64,
    reason_mask: u32,
    return_only_on_close: u32,
    timeout: u64,
    bytes_to_wait_for: u64,
    journal_id: u64,
}

/// A Win32 error code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Win32Error(pub u32);

impl Win32Error {
    fn last() -> Self {
        Win32Error(unsafe { GetLastError() })
    }
}

impl fmt::Display for Win32Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.0 {
            ERROR_ACCESS_DENIED => "ERROR_ACCESS_DENIED",
            ERROR_JOURNAL_DELETE_IN_PROGRESS => "ERROR_JOURNAL_DELETE_IN_PROGRESS",
            ERROR_JOURNAL_NOT_ACTIVE => "ERROR_JOURNAL_NOT_ACTIVE",
            ERROR_JOURNAL_ENTRY_DELETED => "ERROR_JOURNAL_ENTRY_DELETED",
            2 => "ERROR_FILE_NOT_FOUND",
            3 => "ERROR_PATH_NOT_FOUND",
            1 => "ERROR_INVALID_FUNCTION",
            87 => "ERROR_INVALID_PARAMETER",
            _ => "",
        };
        if name.is_empty() {
            write!(f, "Win32 error {}", self.0)
        } else {
            write!(f, "{} ({})", name, self.0)
        }
    }
}

struct OwnedHandle(Handle);

impl OwnedHandle {
    fn new(h: Handle) -> Result<Self, Win32Error> {
        if h == INVALID_HANDLE_VALUE || h.is_null() {
            Err(Win32Error::last())
        } else {
            Ok(OwnedHandle(h))
        }
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

pub fn is_admin() -> bool {
    unsafe { IsUserAnAdmin() != 0 }
}

#[derive(Clone, Debug)]
pub struct DriveInfo {
    pub letter: char,
    pub label: String,
    pub format: String,
    pub kind: &'static str,
    pub total_size: u64,
    pub total_free: u64,
}

impl DriveInfo {
    pub fn root(&self) -> String {
        format!("{}:\\", self.letter)
    }
}

/// Ready NTFS drives, like the C# drive selection form.
pub fn list_ntfs_drives() -> Vec<DriveInfo> {
    let mask = unsafe { GetLogicalDrives() };
    let mut out = Vec::new();
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        let root = wide(&format!("{letter}:\\"));
        let mut label = [0u16; 261];
        let mut fs = [0u16; 261];
        let ok = unsafe {
            GetVolumeInformationW(
                root.as_ptr(),
                label.as_mut_ptr(),
                label.len() as u32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                fs.as_mut_ptr(),
                fs.len() as u32,
            )
        };
        if ok == 0 {
            continue; // not ready
        }
        let format = from_wide(&fs);
        if !format.eq_ignore_ascii_case("ntfs") {
            continue;
        }
        let kind = match unsafe { GetDriveTypeW(root.as_ptr()) } {
            2 => "Removable",
            3 => "Fixed",
            4 => "Network",
            5 => "CDRom",
            6 => "Ram",
            _ => "Unknown",
        };
        let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
        unsafe {
            GetDiskFreeSpaceExW(root.as_ptr(), &mut avail, &mut total, &mut free);
        }
        out.push(DriveInfo {
            letter,
            label: from_wide(&label),
            format,
            kind,
            total_size: total,
            total_free: free,
        });
    }
    out
}

/// Open handles to one volume: the raw volume for journal IO and its root
/// directory as the hint for OpenFileById.
pub struct Volume {
    letter: char,
    volume: OwnedHandle,
    root: OwnedHandle,
}

// Kernel handles can be used from any thread; we only issue independent,
// synchronous calls on them.
unsafe impl Send for Volume {}
unsafe impl Sync for Volume {}

impl Volume {
    pub fn open(letter: char) -> Result<Volume, Win32Error> {
        let path = wide(&format!("\\\\.\\{letter}:"));
        let volume = OwnedHandle::new(unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_ALL,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        })?;
        // The C# version used the process' current directory as the hint,
        // which resolves IDs against the wrong volume when it is not on the
        // scanned drive. Use the scanned volume's root instead.
        let root_path = wide(&format!("{letter}:\\"));
        let root = OwnedHandle::new(unsafe {
            CreateFileW(
                root_path.as_ptr(),
                FILE_READ_ATTRIBUTES,
                FILE_SHARE_ALL,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                std::ptr::null_mut(),
            )
        })?;
        Ok(Volume { letter, volume, root })
    }

    pub fn letter(&self) -> char {
        self.letter
    }

    pub fn query_journal(&self) -> Result<JournalInfo, Win32Error> {
        let mut data = UsnJournalDataV0::default();
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                self.volume.0,
                FSCTL_QUERY_USN_JOURNAL,
                std::ptr::null(),
                0,
                &mut data as *mut _ as *mut c_void,
                std::mem::size_of::<UsnJournalDataV0>() as u32,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(Win32Error::last());
        }
        Ok(JournalInfo {
            journal_id: data.journal_id,
            first_usn: data.first_usn,
            next_usn: data.next_usn,
            lowest_valid_usn: data.lowest_valid_usn,
            max_usn: data.max_usn,
            maximum_size: data.maximum_size,
            allocation_delta: data.allocation_delta,
        })
    }

    /// Reads every record from `FirstUsn` up to the `NextUsn` snapshot in
    /// `info`. `progress` gets a 0..1 fraction now and then.
    pub fn read_entries(&self, info: &JournalInfo, progress: impl FnMut(f32)) -> Result<Vec<Entry>, Win32Error> {
        self.read_range(info.journal_id, info.first_usn, info.next_usn, info.lowest_valid_usn, progress)
    }

    /// Reads records with `start <= USN < end`. Live monitoring calls this
    /// with the previous `NextUsn` as `start`.
    pub fn read_range(
        &self,
        journal_id: u64,
        start: i64,
        end: i64,
        lowest_valid: i64,
        mut progress: impl FnMut(f32),
    ) -> Result<Vec<Entry>, Win32Error> {
        const BUF_BYTES: usize = 1 << 20;
        let mut buf = vec![0u64; BUF_BYTES / 8]; // u64 for 8-byte alignment
        let mut entries = Vec::new();
        let mut req = ReadUsnJournalDataV0 {
            start_usn: start,
            reason_mask: 0xFFFF_FFFF,
            return_only_on_close: 0,
            timeout: 0,
            bytes_to_wait_for: 0,
            journal_id,
        };
        let span = (end - start).max(1) as f64;

        loop {
            let mut returned = 0u32;
            let ok = unsafe {
                DeviceIoControl(
                    self.volume.0,
                    FSCTL_READ_USN_JOURNAL,
                    &req as *const _ as *const c_void,
                    std::mem::size_of::<ReadUsnJournalDataV0>() as u32,
                    buf.as_mut_ptr() as *mut c_void,
                    BUF_BYTES as u32,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                let err = Win32Error::last();
                if err.0 == ERROR_HANDLE_EOF {
                    break;
                }
                // The oldest records can be purged while we read; skip ahead.
                if err.0 == ERROR_JOURNAL_ENTRY_DELETED && req.start_usn < lowest_valid {
                    req.start_usn = lowest_valid;
                    continue;
                }
                return Err(err);
            }

            let bytes = unsafe {
                std::slice::from_raw_parts(buf.as_ptr() as *const u8, (returned as usize).min(BUF_BYTES))
            };
            if bytes.len() < 8 {
                break;
            }
            let next_usn = i64::from_le_bytes(bytes[0..8].try_into().unwrap());

            let mut off = 8;
            let mut reached_end = false;
            while off + 8 <= bytes.len() {
                let len = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
                if len == 0 || off + len > bytes.len() {
                    break;
                }
                if let Some(entry) = parse_record(&bytes[off..off + len]) {
                    if entry.usn >= end {
                        reached_end = true;
                        break;
                    }
                    entries.push(entry);
                }
                off += len;
            }

            progress((((next_usn - start) as f64) / span).clamp(0.0, 1.0) as f32);

            if reached_end || next_usn >= end || next_usn <= req.start_usn {
                break;
            }
            req.start_usn = next_usn;
        }
        Ok(entries)
    }

    /// Current full path of a file reference, or None when it no longer exists.
    pub fn resolve_path(&self, file_ref: u64) -> Option<String> {
        let desc = FileIdDescriptor {
            size: std::mem::size_of::<FileIdDescriptor>() as u32,
            kind: 0,
            file_id: file_ref,
            _pad: 0,
        };
        // Zero access: we only need the name, and it avoids ACL denials.
        let handle = OwnedHandle::new(unsafe {
            OpenFileById(
                self.root.0,
                &desc,
                0,
                FILE_SHARE_ALL,
                std::ptr::null_mut(),
                FILE_FLAG_BACKUP_SEMANTICS,
            )
        })
        .ok()?;

        // The C# version used a fixed 128 char buffer and cut long paths.
        let mut buf = vec![0u16; 512];
        loop {
            let n = unsafe { GetFinalPathNameByHandleW(handle.0, buf.as_mut_ptr(), buf.len() as u32, 0) }
                as usize;
            if n == 0 {
                return None;
            }
            if n < buf.len() {
                let path = String::from_utf16_lossy(&buf[..n]);
                return Some(strip_verbatim(path));
            }
            buf.resize(n + 1, 0);
        }
    }
}

fn strip_verbatim(path: String) -> String {
    if let Some(rest) = path.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{rest}")
    } else if let Some(rest) = path.strip_prefix("\\\\?\\") {
        rest.to_string()
    } else {
        path
    }
}

fn u16_at(b: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(off..off + 2)?.try_into().ok()?))
}
fn u32_at(b: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(off..off + 4)?.try_into().ok()?))
}
fn u64_at(b: &[u8], off: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(off..off + 8)?.try_into().ok()?))
}

/// Parses a USN_RECORD_V2 or V3. V3 file IDs are 128 bit; NTFS only uses the
/// low 64 bits, which is what OpenFileById(FileIdType) takes.
fn parse_record(r: &[u8]) -> Option<Entry> {
    let major = u16_at(r, 4)?;
    let (file_ref, parent_ref, usn, ts, reason, attrs, name_len, name_off) = match major {
        2 => (
            u64_at(r, 8)?,
            u64_at(r, 16)?,
            u64_at(r, 24)? as i64,
            u64_at(r, 32)? as i64,
            u32_at(r, 40)?,
            u32_at(r, 52)?,
            u16_at(r, 56)? as usize,
            u16_at(r, 58)? as usize,
        ),
        3 => (
            u64_at(r, 8)?,
            u64_at(r, 24)?,
            u64_at(r, 40)? as i64,
            u64_at(r, 48)? as i64,
            u32_at(r, 56)?,
            u32_at(r, 68)?,
            u16_at(r, 72)? as usize,
            u16_at(r, 74)? as usize,
        ),
        _ => return None,
    };
    let raw = r.get(name_off..name_off + name_len)?;
    let units: Vec<u16> = raw.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
    Some(Entry {
        usn,
        name: String::from_utf16_lossy(&units),
        file_ref,
        parent_ref,
        timestamp: ts,
        reason,
        attributes: attrs,
        vol: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v2_record(name: &str) -> Vec<u8> {
        let units: Vec<u16> = name.encode_utf16().collect();
        let len = (60 + units.len() * 2 + 7) & !7;
        let mut r = vec![0u8; len];
        r[0..4].copy_from_slice(&(len as u32).to_le_bytes());
        r[4..6].copy_from_slice(&2u16.to_le_bytes());
        r[8..16].copy_from_slice(&11u64.to_le_bytes());
        r[16..24].copy_from_slice(&5u64.to_le_bytes());
        r[24..32].copy_from_slice(&1234i64.to_le_bytes());
        r[32..40].copy_from_slice(&132_000_000_000_000_000i64.to_le_bytes());
        r[40..44].copy_from_slice(&0x8000_0200u32.to_le_bytes());
        r[56..58].copy_from_slice(&((units.len() * 2) as u16).to_le_bytes());
        r[58..60].copy_from_slice(&60u16.to_le_bytes());
        for (i, u) in units.iter().enumerate() {
            r[60 + i * 2..62 + i * 2].copy_from_slice(&u.to_le_bytes());
        }
        r
    }

    #[test]
    fn parses_v2() {
        let e = parse_record(&v2_record("çalış.exe")).unwrap();
        assert_eq!(e.name, "çalış.exe");
        assert_eq!(e.file_ref, 11);
        assert_eq!(e.parent_ref, 5);
        assert_eq!(e.usn, 1234);
        assert_eq!(e.reason, 0x8000_0200);
    }

    #[test]
    fn rejects_truncated() {
        let r = v2_record("abc");
        assert!(parse_record(&r[..40]).is_none());
        let mut bad = r.clone();
        bad[58..60].copy_from_slice(&500u16.to_le_bytes());
        assert!(parse_record(&bad).is_none());
    }

    #[test]
    fn strips_prefix() {
        assert_eq!(strip_verbatim("\\\\?\\C:\\Windows".into()), "C:\\Windows");
        assert_eq!(strip_verbatim("\\\\?\\UNC\\srv\\s".into()), "\\\\srv\\s");
    }
}
