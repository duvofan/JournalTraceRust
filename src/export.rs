//! Export of the rows currently shown: CSV, JSON and a self-contained HTML report.
//!
//! All writers stream through a `BufWriter` and return the number of data rows
//! written.

use std::borrow::Cow;
use std::fs::File;
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

/// USN reason bits used for the summary and the colouring.
const REASON_CREATE: u32 = 0x0000_0100;
const REASON_DELETE: u32 = 0x0000_0200;
const REASON_RENAME: u32 = 0x0000_1000 | 0x0000_2000;

/// One exported row. `reason_mask` is the raw USN reason bitmask.
pub struct ExportRow<'a> {
    pub usn: i64,
    pub drive: char,
    pub name: &'a str,
    pub date: Cow<'a, str>,
    pub reason: &'a str,
    pub directory: &'a str,
    pub reason_mask: u32,
}

/// Column titles in the user's language, in this order:
/// USN, Drive, Name, Date, Reason, Directory.
pub type Header<'a> = [&'a str; 6];

// ---------------------------------------------------------------------------
// CSV
// ---------------------------------------------------------------------------

/// UTF-8 with BOM (so Excel picks the right encoding), CRLF line ends, RFC 4180 quoting.
pub fn write_csv<'a>(path: &Path, header: Header<'_>, rows: impl IntoIterator<Item = ExportRow<'a>>) -> io::Result<usize> {
    let mut w = BufWriter::new(File::create(path)?);
    w.write_all(b"\xEF\xBB\xBF")?;
    csv_line(&mut w, header.iter().map(|s| Cow::Borrowed(*s)))?;
    let mut n = 0;
    let mut usn = String::new();
    for r in rows {
        usn.clear();
        usn.push_str(&r.usn.to_string());
        let mut drive = [0u8; 4];
        let drive: &str = r.drive.encode_utf8(&mut drive);
        csv_cell(&mut w, &usn)?;
        w.write_all(b",")?;
        csv_cell(&mut w, drive)?;
        w.write_all(b",")?;
        csv_cell(&mut w, r.name)?;
        w.write_all(b",")?;
        csv_cell(&mut w, &r.date)?;
        w.write_all(b",")?;
        csv_cell(&mut w, r.reason)?;
        w.write_all(b",")?;
        csv_cell(&mut w, r.directory)?;
        w.write_all(b"\r\n")?;
        n += 1;
    }
    w.flush()?;
    Ok(n)
}

fn csv_line<'a>(w: &mut impl Write, cells: impl Iterator<Item = Cow<'a, str>>) -> io::Result<()> {
    for (i, cell) in cells.enumerate() {
        if i > 0 {
            w.write_all(b",")?;
        }
        csv_cell(w, &cell)?;
    }
    w.write_all(b"\r\n")
}

fn csv_cell(w: &mut impl Write, cell: &str) -> io::Result<()> {
    if !cell.bytes().any(|b| matches!(b, b',' | b'"' | b'\r' | b'\n')) {
        return w.write_all(cell.as_bytes());
    }
    w.write_all(b"\"")?;
    let b = cell.as_bytes();
    let mut start = 0;
    for (i, &c) in b.iter().enumerate() {
        if c == b'"' {
            // Write up to and including the quote, then the doubled quote.
            w.write_all(&b[start..=i])?;
            w.write_all(b"\"")?;
            start = i + 1;
        }
    }
    w.write_all(&b[start..])?;
    w.write_all(b"\"")
}

// ---------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------

/// A single JSON array with one object per line, written by hand.
pub fn write_json<'a>(path: &Path, rows: impl IntoIterator<Item = ExportRow<'a>>) -> io::Result<usize> {
    let mut w = BufWriter::new(File::create(path)?);
    w.write_all(b"[")?;
    let mut n = 0;
    for r in rows {
        w.write_all(if n == 0 { b"\n" } else { b",\n" })?;
        write!(w, "{{\"usn\":{},\"drive\":", r.usn)?;
        let mut drive = [0u8; 4];
        json_str(&mut w, r.drive.encode_utf8(&mut drive))?;
        w.write_all(b",\"name\":")?;
        json_str(&mut w, r.name)?;
        w.write_all(b",\"date\":")?;
        json_str(&mut w, &r.date)?;
        w.write_all(b",\"reason\":")?;
        json_str(&mut w, r.reason)?;
        w.write_all(b",\"directory\":")?;
        json_str(&mut w, r.directory)?;
        write!(w, ",\"reason_mask\":{}}}", r.reason_mask)?;
        n += 1;
    }
    w.write_all(b"\n]\n")?;
    w.flush()?;
    Ok(n)
}

/// Writes `s` as a quoted JSON string.
fn json_str(w: &mut impl Write, s: &str) -> io::Result<()> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    w.write_all(b"\"")?;
    let b = s.as_bytes();
    let mut start = 0;
    for (i, &c) in b.iter().enumerate() {
        let short: &[u8] = match c {
            b'"' => b"\\\"",
            b'\\' => b"\\\\",
            b'\n' => b"\\n",
            b'\r' => b"\\r",
            b'\t' => b"\\t",
            0..=0x1f => &[b'\\', b'u', b'0', b'0', HEX[usize::from(c >> 4)], HEX[usize::from(c & 15)]],
            _ => continue,
        };
        w.write_all(&b[start..i])?;
        w.write_all(short)?;
        start = i + 1;
    }
    w.write_all(&b[start..])?;
    w.write_all(b"\"")
}

// ---------------------------------------------------------------------------
// HTML
// ---------------------------------------------------------------------------

pub struct HtmlLabels<'a> {
    /// Page title, e.g. "JournalTrace".
    pub title: &'a str,
    /// e.g. "Generated:" (followed by `generated_at`).
    pub generated: &'a str,
    /// Already formatted time text.
    pub generated_at: &'a str,
    /// e.g. "Rows:".
    pub rows: &'a str,
    /// Summary labels.
    pub deleted: &'a str,
    pub created: &'a str,
    pub renamed: &'a str,
    /// Placeholder of the filter box.
    pub search: &'a str,
    pub header: Header<'a>,
}

/// Width reserved for each summary number; patched in place once the rows have been counted.
const COUNT_WIDTH: usize = 20;

/// Self-contained HTML report (inline CSS and JS, no external resources) with
/// a live filter box and sortable columns.
pub fn write_html<'a>(path: &Path, labels: &HtmlLabels<'_>, rows: impl IntoIterator<Item = ExportRow<'a>>) -> io::Result<usize> {
    let mut w = BufWriter::new(File::create(path)?);

    w.write_all(HTML_HEAD_1.as_bytes())?;
    html_esc(&mut w, labels.title)?;
    w.write_all(HTML_HEAD_2.as_bytes())?;

    w.write_all(b"<body>\n<header>\n<h1>")?;
    html_esc(&mut w, labels.title)?;
    w.write_all(b"</h1>\n<div class=\"gen\">")?;
    html_esc(&mut w, labels.generated)?;
    w.write_all(b" ")?;
    html_esc(&mut w, labels.generated_at)?;
    w.write_all(b"</div>\n<div class=\"chips\">\n")?;

    // The totals are only known after the last row, so reserve space for them and patch it afterwards.
    let mut slots = [0u64; 4];
    let chips: [(&str, &str); 4] = [("chip", labels.rows), ("chip d", labels.deleted), ("chip c", labels.created), ("chip r", labels.renamed)];
    for (slot, (class, label)) in slots.iter_mut().zip(chips) {
        write!(w, "<span class=\"{class}\">")?;
        html_esc(&mut w, label)?;
        w.write_all(b" <b>")?;
        *slot = w.stream_position()?;
        w.write_all(&[b' '; COUNT_WIDTH])?;
        w.write_all(b"</b></span>\n")?;
    }

    w.write_all(b"</div>\n<div class=\"find\"><input id=\"q\" type=\"search\" autocomplete=\"off\" spellcheck=\"false\" placeholder=\"")?;
    html_esc(&mut w, labels.search)?;
    w.write_all(b"\"><span id=\"cnt\"></span></div>\n</header>\n<main>\n<table id=\"t\">\n<thead><tr>")?;
    for (i, title) in labels.header.iter().enumerate() {
        let class = match i {
            0 => " class=\"usn\"",
            1 => " class=\"drv\"",
            _ => "",
        };
        write!(w, "<th{class} data-c=\"{i}\">")?;
        html_esc(&mut w, title)?;
        w.write_all(b"</th>")?;
    }
    w.write_all(b"</tr></thead>\n<tbody>\n")?;

    let mut counts = [0u64; 4]; // rows, deleted, created, renamed
    for r in rows {
        counts[0] += 1;
        if r.reason_mask & REASON_DELETE != 0 {
            counts[1] += 1;
        }
        if r.reason_mask & REASON_CREATE != 0 {
            counts[2] += 1;
        }
        if r.reason_mask & REASON_RENAME != 0 {
            counts[3] += 1;
        }
        // Precedence when several bits are set: delete, create, rename.
        let class = if r.reason_mask & REASON_DELETE != 0 {
            " class=\"d\""
        } else if r.reason_mask & REASON_CREATE != 0 {
            " class=\"c\""
        } else if r.reason_mask & REASON_RENAME != 0 {
            " class=\"r\""
        } else {
            ""
        };
        write!(w, "<tr{class}><td class=\"usn\">{}</td><td class=\"drv\">", r.usn)?;
        let mut drive = [0u8; 4];
        html_esc(&mut w, r.drive.encode_utf8(&mut drive))?;
        w.write_all(b"</td><td>")?;
        html_esc(&mut w, r.name)?;
        w.write_all(b"</td><td class=\"dt\">")?;
        html_esc(&mut w, &r.date)?;
        w.write_all(b"</td><td>")?;
        html_esc(&mut w, r.reason)?;
        w.write_all(b"</td><td>")?;
        html_esc(&mut w, r.directory)?;
        w.write_all(b"</td></tr>\n")?;
    }

    w.write_all(b"</tbody>\n</table>\n</main>\n<script>\n")?;
    w.write_all(HTML_JS.as_bytes())?;
    w.write_all(b"</script>\n</body>\n</html>\n")?;

    // Fill in the reserved summary numbers (the field stays COUNT_WIDTH bytes wide).
    for (pos, count) in slots.iter().zip(counts) {
        w.seek(SeekFrom::Start(*pos))?;
        let text = count.to_string();
        debug_assert!(text.len() <= COUNT_WIDTH);
        w.write_all(text.as_bytes())?;
    }
    w.flush()?;
    Ok(counts[0] as usize)
}

/// Writes `s` with `& < > " '` replaced by entities.
fn html_esc(w: &mut impl Write, s: &str) -> io::Result<()> {
    let b = s.as_bytes();
    let mut start = 0;
    for (i, &c) in b.iter().enumerate() {
        let rep: &[u8] = match c {
            b'&' => b"&amp;",
            b'<' => b"&lt;",
            b'>' => b"&gt;",
            b'"' => b"&quot;",
            b'\'' => b"&#39;",
            _ => continue,
        };
        w.write_all(&b[start..i])?;
        w.write_all(rep)?;
        start = i + 1;
    }
    w.write_all(&b[start..])
}

// Everything between <!DOCTYPE> and the page title.
const HTML_HEAD_1: &str = "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'\">\n\
<title>";

// Closing of the title, then the style sheet.
const HTML_HEAD_2: &str = r#"</title>
<style>
:root{color-scheme:dark}
*{box-sizing:border-box}
html,body{height:100%;margin:0}
body{display:flex;flex-direction:column;background:#1e1e1e;color:#d4d4d4;font:13px/1.4 "Segoe UI",system-ui,-apple-system,sans-serif}
header{flex:none;padding:14px 18px 10px;background:#252526;border-bottom:1px solid #3c3c3c}
h1{margin:0 0 2px;font-size:20px;font-weight:600;color:#fff}
.gen{color:#9d9d9d;margin-bottom:10px}
.chips{display:flex;flex-wrap:wrap;gap:8px;margin-bottom:10px}
.chip{padding:3px 10px;border:1px solid #3c3c3c;border-radius:12px;background:#2d2d30;white-space:pre}
.chip b{color:#fff;font-weight:600}
.chip.d{color:#ff7b72}.chip.c{color:#7ee787}.chip.r{color:#e3b341}
.find{display:flex;align-items:center;gap:12px}
#q{width:380px;max-width:100%;padding:6px 10px;color:#d4d4d4;background:#1e1e1e;border:1px solid #3c3c3c;border-radius:3px;font:inherit;outline:none}
#q:focus{border-color:#0078d7;box-shadow:0 0 0 1px #0078d7}
#cnt{color:#9d9d9d}
main{flex:1;min-height:0;overflow:auto}
table{width:100%;border-collapse:collapse}
th{position:sticky;top:0;z-index:1;padding:7px 10px;text-align:left;font-weight:600;color:#fff;background:#333337;border-bottom:2px solid #0078d7;cursor:pointer;user-select:none;white-space:nowrap}
th:hover{background:#3e3e42}
th.asc::after{content:" \25B2";color:#0078d7;font-size:10px}
th.desc::after{content:" \25BC";color:#0078d7;font-size:10px}
td{padding:3px 10px;border-bottom:1px solid #2a2a2a;vertical-align:top;word-break:break-word}
tbody tr:hover td{background:#2a2d2e}
td.usn,th.usn{text-align:right;font-family:Consolas,"Cascadia Mono",monospace;white-space:nowrap}
td.drv,th.drv{text-align:center}
td.dt{white-space:nowrap}
tr.d td{color:#ff7b72}
tr.c td{color:#7ee787}
tr.r td{color:#e3b341}
tr[hidden]{display:none}
</style>
</head>
"#;

const HTML_JS: &str = r#"(function () {
'use strict';
var tbody = document.getElementById('t').tBodies[0];
var trs = Array.prototype.slice.call(tbody.rows);
var total = trs.length;
var keys = new Array(total);          // lowercase text of each row, built once
var hidden = new Array(total);
var cols = trs.length ? trs[0].cells.length : 0;
var sortCache = {};
var order = [];                       // order[k] = index into trs shown at position k
var i, j;
for (i = 0; i < total; i++) {
  var cells = trs[i].cells, s = '';
  for (j = 0; j < cells.length; j++) { s += cells[j].textContent + '\n'; }
  keys[i] = s.toLowerCase();
  hidden[i] = false;
  order.push(i);
}

var cnt = document.getElementById('cnt');
var box = document.getElementById('q');
var timer = 0;

function showCount(n) { cnt.textContent = n === total ? String(total) : n + ' / ' + total; }

function applyFilter() {
  var q = box.value.toLowerCase();
  var n = 0, k, hide;
  for (k = 0; k < total; k++) {
    hide = q !== '' && keys[k].indexOf(q) < 0;
    if (hide !== hidden[k]) { hidden[k] = hide; trs[k].hidden = hide; }
    if (!hide) { n++; }
  }
  showCount(n);
}

box.addEventListener('input', function () {
  clearTimeout(timer);
  timer = setTimeout(applyFilter, 120);
});

function toNum(t) {
  try { return BigInt(t); } catch (e) { return parseFloat(t) || 0; }
}

var dateRe = /^(\d\d)\/(\d\d)\/(\d{4,}) (\d\d):(\d\d):(\d\d)/;
function dateKey(t) {
  var m = dateRe.exec(t);
  return m ? m[3] + m[2] + m[1] + m[4] + m[5] + m[6] : t.toLowerCase();
}

function sortKeys(c) {
  if (sortCache[c]) { return sortCache[c]; }
  var out = new Array(total), k, t;
  for (k = 0; k < total; k++) {
    t = trs[k].cells[c].textContent;
    out[k] = c === 0 ? toNum(t) : c === 3 ? dateKey(t) : t.toLowerCase();
  }
  sortCache[c] = out;
  return out;
}

var heads = document.querySelectorAll('#t th');
var cur = -1, dir = 1;
function onSort(c) {
  dir = (c === cur) ? -dir : 1;
  cur = c;
  var v = sortKeys(c);
  order.sort(function (a, b) {
    var x = v[a], y = v[b];
    return ((x > y) - (x < y)) * dir || a - b;
  });
  var frag = document.createDocumentFragment(), k;
  for (k = 0; k < total; k++) { frag.appendChild(trs[order[k]]); }
  tbody.appendChild(frag);
  for (k = 0; k < heads.length; k++) { heads[k].className = heads[k].className.replace(/\s*\b(asc|desc)\b/g, ''); }
  heads[c].className += dir > 0 ? ' asc' : ' desc';
}
Array.prototype.forEach.call(heads, function (th, c) {
  th.addEventListener('click', function () { onSort(c); });
});

showCount(total);
})();
"#;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Temp file that is deleted on drop, unique per call.
    struct TempFile(std::path::PathBuf);
    impl TempFile {
        fn new(ext: &str) -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            TempFile(std::env::temp_dir().join(format!("jt_export_test_{}_{n}.{ext}", std::process::id())))
        }
        fn read(&self) -> Vec<u8> {
            std::fs::read(&self.0).unwrap()
        }
        fn text(&self) -> String {
            String::from_utf8(self.read()).unwrap()
        }
    }
    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    const HEADER: Header<'static> = ["USN", "Drive", "Name", "Date", "Reason", "Directory"];

    fn row<'a>(usn: i64, name: &'a str, reason: &'a str, mask: u32) -> ExportRow<'a> {
        ExportRow { usn, drive: 'C', name, date: Cow::Borrowed("01/02/2024 03:04:05"), reason, directory: "C:\\Dir", reason_mask: mask }
    }

    fn labels() -> HtmlLabels<'static> {
        HtmlLabels {
            title: "JournalTrace",
            generated: "Generated:",
            generated_at: "01/02/2024 10:00:00",
            rows: "Rows:",
            deleted: "Deleted:",
            created: "Created:",
            renamed: "Renamed:",
            search: "Filter...",
            header: HEADER,
        }
    }

    // ---- CSV ----

    #[test]
    fn csv_bom_header_crlf_and_columns() {
        let f = TempFile::new("csv");
        let n = write_csv(&f.0, HEADER, vec![row(42, "a.txt", "File delete", 0x200)]).unwrap();
        assert_eq!(n, 1);
        let bytes = f.read();
        assert_eq!(&bytes[..3], b"\xEF\xBB\xBF");
        let text = String::from_utf8(bytes[3..].to_vec()).unwrap();
        assert_eq!(text, "USN,Drive,Name,Date,Reason,Directory\r\n42,C,a.txt,01/02/2024 03:04:05,File delete,C:\\Dir\r\n");
    }

    #[test]
    fn csv_quoting() {
        let f = TempFile::new("csv");
        let rows = vec![
            row(1, "a,\"b\"", "plain", 0),
            row(2, "line1\nline2", "cr\rcr", 0),
            row(3, "\"", "", 0),
            row(4, "ünï cødé 日本", "x", 0),
        ];
        assert_eq!(write_csv(&f.0, HEADER, rows).unwrap(), 4);
        let text = f.text();
        assert!(text.contains("1,C,\"a,\"\"b\"\"\",01/02/2024 03:04:05,plain,C:\\Dir\r\n"), "{text:?}");
        assert!(text.contains("2,C,\"line1\nline2\",01/02/2024 03:04:05,\"cr\rcr\",C:\\Dir\r\n"), "{text:?}");
        assert!(text.contains("3,C,\"\"\"\",01/02/2024 03:04:05,,C:\\Dir\r\n"), "{text:?}");
        assert!(text.contains("4,C,ünï cødé 日本,"), "{text:?}");
    }

    #[test]
    fn csv_quotes_header_cells_and_handles_empty() {
        let f = TempFile::new("csv");
        let header = ["U,SN", "Drive", "Na\"me", "Date", "Reason", "Directory"];
        assert_eq!(write_csv(&f.0, header, Vec::new()).unwrap(), 0);
        assert_eq!(f.text(), "\u{feff}\"U,SN\",Drive,\"Na\"\"me\",Date,Reason,Directory\r\n");
    }

    #[test]
    fn csv_negative_usn_and_odd_drive() {
        let f = TempFile::new("csv");
        let mut r = row(-5, "n", "r", 0);
        r.drive = '"';
        assert_eq!(write_csv(&f.0, HEADER, [r]).unwrap(), 1);
        assert!(f.text().contains("-5,\"\"\"\",n,"));
    }

    // ---- JSON ----

    #[test]
    fn json_exact_output() {
        let f = TempFile::new("json");
        let mut r2 = row(-7, "q\"b\\s", "r", 0xFFFF_FFFF);
        r2.drive = 'D';
        let n = write_json(&f.0, vec![row(1, "a.txt", "File create", 0x100), r2]).unwrap();
        assert_eq!(n, 2);
        assert_eq!(
            f.text(),
            "[\n{\"usn\":1,\"drive\":\"C\",\"name\":\"a.txt\",\"date\":\"01/02/2024 03:04:05\",\"reason\":\"File create\",\"directory\":\"C:\\\\Dir\",\"reason_mask\":256},\n\
             {\"usn\":-7,\"drive\":\"D\",\"name\":\"q\\\"b\\\\s\",\"date\":\"01/02/2024 03:04:05\",\"reason\":\"r\",\"directory\":\"C:\\\\Dir\",\"reason_mask\":4294967295}\n]\n"
        );
    }

    #[test]
    fn json_empty_is_valid() {
        let f = TempFile::new("json");
        assert_eq!(write_json(&f.0, Vec::new()).unwrap(), 0);
        let v = json::parse(&f.text()).unwrap();
        assert_eq!(v, json::Value::Array(vec![]));
    }

    #[test]
    fn json_escapes_control_chars() {
        let mut out = Vec::new();
        json_str(&mut out, "a\"b\\c\nd\re\tf\u{0}g\u{1f}h\u{8}\u{c}\u{7f}/é日\u{2028}").unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "\"a\\\"b\\\\c\\nd\\re\\tf\\u0000g\\u001fh\\u0008\\u000c\u{7f}/é日\u{2028}\"");
    }

    #[test]
    fn json_roundtrip_through_parser() {
        let nasty = "a\"b\\c\nd\re\tf\u{0}g\u{1f}h/é日😀</script>";
        let f = TempFile::new("json");
        let mut r = row(i64::MIN, nasty, nasty, u32::MAX);
        r.drive = '日';
        r.directory = nasty;
        let rows = vec![r, row(i64::MAX, "x", "y", 0)];
        assert_eq!(write_json(&f.0, rows).unwrap(), 2);
        let text = f.text();
        // One object per line.
        assert_eq!(text.lines().count(), 4);
        let json::Value::Array(items) = json::parse(&text).unwrap() else { panic!("not an array") };
        assert_eq!(items.len(), 2);
        let obj = |v: &json::Value| match v {
            json::Value::Object(o) => o.clone(),
            _ => panic!("not an object"),
        };
        let o = obj(&items[0]);
        let keys: Vec<&str> = o.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["usn", "drive", "name", "date", "reason", "directory", "reason_mask"]);
        assert_eq!(o[0].1, json::Value::Number("-9223372036854775808".into()));
        assert_eq!(o[1].1, json::Value::String("日".into()));
        assert_eq!(o[2].1, json::Value::String(nasty.into()));
        assert_eq!(o[4].1, json::Value::String(nasty.into()));
        assert_eq!(o[5].1, json::Value::String(nasty.into()));
        assert_eq!(o[6].1, json::Value::Number("4294967295".into()));
        assert_eq!(obj(&items[1])[0].1, json::Value::Number("9223372036854775807".into()));
    }

    /// Tiny strict JSON parser, only for checking the output above.
    mod json {
        #[derive(Debug, Clone, PartialEq)]
        pub enum Value {
            Number(String),
            String(String),
            Array(Vec<Value>),
            Object(Vec<(String, Value)>),
        }

        pub fn parse(s: &str) -> Result<Value, String> {
            let mut p = P { b: s.as_bytes(), i: 0 };
            let v = p.value()?;
            p.ws();
            if p.i != p.b.len() {
                return Err(format!("trailing data at {}", p.i));
            }
            Ok(v)
        }

        struct P<'a> {
            b: &'a [u8],
            i: usize,
        }

        impl P<'_> {
            fn ws(&mut self) {
                while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\n' | b'\r' | b'\t') {
                    self.i += 1;
                }
            }
            fn eat(&mut self, c: u8) -> Result<(), String> {
                self.ws();
                if self.b.get(self.i) == Some(&c) {
                    self.i += 1;
                    Ok(())
                } else {
                    Err(format!("expected {:?} at {}", c as char, self.i))
                }
            }
            fn value(&mut self) -> Result<Value, String> {
                self.ws();
                match self.b.get(self.i) {
                    Some(b'[') => {
                        self.i += 1;
                        let mut v = Vec::new();
                        self.ws();
                        if self.b.get(self.i) == Some(&b']') {
                            self.i += 1;
                            return Ok(Value::Array(v));
                        }
                        loop {
                            v.push(self.value()?);
                            self.ws();
                            match self.b.get(self.i) {
                                Some(b',') => self.i += 1,
                                Some(b']') => {
                                    self.i += 1;
                                    return Ok(Value::Array(v));
                                }
                                _ => return Err(format!("bad array at {}", self.i)),
                            }
                        }
                    }
                    Some(b'{') => {
                        self.i += 1;
                        let mut v = Vec::new();
                        loop {
                            self.ws();
                            let k = self.string()?;
                            self.eat(b':')?;
                            v.push((k, self.value()?));
                            self.ws();
                            match self.b.get(self.i) {
                                Some(b',') => self.i += 1,
                                Some(b'}') => {
                                    self.i += 1;
                                    return Ok(Value::Object(v));
                                }
                                _ => return Err(format!("bad object at {}", self.i)),
                            }
                        }
                    }
                    Some(b'"') => Ok(Value::String(self.string()?)),
                    Some(b'-' | b'0'..=b'9') => {
                        let st = self.i;
                        self.i += 1;
                        while self.i < self.b.len() && self.b[self.i].is_ascii_digit() {
                            self.i += 1;
                        }
                        Ok(Value::Number(String::from_utf8(self.b[st..self.i].to_vec()).unwrap()))
                    }
                    _ => Err(format!("unexpected input at {}", self.i)),
                }
            }
            fn string(&mut self) -> Result<String, String> {
                if self.b.get(self.i) != Some(&b'"') {
                    return Err(format!("expected string at {}", self.i));
                }
                self.i += 1;
                let mut out: Vec<u8> = Vec::new();
                loop {
                    let c = *self.b.get(self.i).ok_or("unterminated string")?;
                    self.i += 1;
                    match c {
                        b'"' => return String::from_utf8(out).map_err(|e| e.to_string()),
                        0..=0x1f => return Err(format!("raw control char {c:#x} in string")),
                        b'\\' => {
                            let e = *self.b.get(self.i).ok_or("bad escape")?;
                            self.i += 1;
                            match e {
                                b'"' | b'\\' | b'/' => out.push(e),
                                b'n' => out.push(b'\n'),
                                b'r' => out.push(b'\r'),
                                b't' => out.push(b'\t'),
                                b'b' => out.push(8),
                                b'f' => out.push(12),
                                b'u' => {
                                    let hex = std::str::from_utf8(self.b.get(self.i..self.i + 4).ok_or("bad \\u")?).map_err(|e| e.to_string())?;
                                    let cp = u32::from_str_radix(hex, 16).map_err(|e| e.to_string())?;
                                    self.i += 4;
                                    let ch = char::from_u32(cp).ok_or("surrogate")?;
                                    let mut buf = [0u8; 4];
                                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                                }
                                _ => return Err(format!("bad escape \\{}", e as char)),
                            }
                        }
                        _ => out.push(c),
                    }
                }
            }
        }
    }

    // ---- HTML ----

    #[test]
    fn html_escaping_of_malicious_name() {
        let f = TempFile::new("html");
        let evil = "<script>alert(1)</script>.exe";
        let evil2 = "\"><img src=x onerror='alert(2)'>&amp;";
        let mut r = row(1, evil, evil2, 0x200);
        r.directory = "C:\\<b>";
        let mut lb = labels();
        lb.title = "<i>t&t</i>";
        lb.search = "\" onfocus=\"alert(3)";
        lb.generated_at = "<x>";
        assert_eq!(write_html(&f.0, &lb, vec![r]).unwrap(), 1);
        let html = f.text();

        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;.exe"));
        assert!(html.contains("&quot;&gt;&lt;img src=x onerror=&#39;alert(2)&#39;&gt;&amp;amp;"));
        assert!(html.contains("C:\\&lt;b&gt;"));
        assert!(html.contains("<title>&lt;i&gt;t&amp;t&lt;/i&gt;</title>"));
        assert!(html.contains("placeholder=\"&quot; onfocus=&quot;alert(3)\""));
        assert!(html.contains("&lt;x&gt;"));
        // The only script tag is ours, and no payload survived unescaped.
        assert!(!html.contains("alert(1)</script>"));
        assert!(!html.contains("<img"));
        assert!(!html.contains("C:\\<b>"));
        assert!(!html.contains("<i>"));
        assert!(!html.contains("<x>"));
        assert_eq!(html.matches("<script").count(), 1);
        assert_eq!(html.matches("</script>").count(), 1);
    }

    #[test]
    fn html_is_self_contained() {
        let f = TempFile::new("html");
        assert_eq!(write_html(&f.0, &labels(), vec![row(1, "a", "b", 0)]).unwrap(), 1);
        let html = f.text();
        for bad in ["http://", "https://", "src=", "href=", "@import", "url(", "<link", "<iframe"] {
            assert!(!html.contains(bad), "unexpected {bad}");
        }
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("#1e1e1e") && html.contains("#d4d4d4") && html.contains("#0078d7"));
        assert!(html.contains("#ff7b72") && html.contains("#7ee787") && html.contains("#e3b341"));
        assert!(html.trim_end().ends_with("</html>"));
    }

    #[test]
    fn html_counts_and_classes() {
        let f = TempFile::new("html");
        let rows = vec![
            row(1, "del", "Delete", 0x200),
            row(2, "cre", "Create", 0x100),
            row(3, "ren-old", "Rename old", 0x1000),
            row(4, "ren-new", "Rename new", 0x2000),
            row(5, "plain", "Data", 0x2),
            row(6, "all", "Create+Delete+Rename", 0x1300),
        ];
        assert_eq!(write_html(&f.0, &labels(), rows).unwrap(), 6);
        let html = f.text();

        let chip = |class: &str, label: &str| -> String {
            let start = html.find(&format!("<span class=\"{class}\">{label} <b>")).unwrap_or_else(|| panic!("chip {label}"));
            let rest = &html[start..];
            let b0 = rest.find("<b>").unwrap() + 3;
            let b1 = rest.find("</b>").unwrap();
            let field = &rest[b0..b1];
            assert_eq!(field.len(), COUNT_WIDTH, "reserved width must be preserved");
            field.trim().to_string()
        };
        assert_eq!(chip("chip", "Rows:"), "6");
        assert_eq!(chip("chip d", "Deleted:"), "2");
        assert_eq!(chip("chip c", "Created:"), "2");
        assert_eq!(chip("chip r", "Renamed:"), "3");

        assert_eq!(html.matches("<tr class=\"d\">").count(), 2);
        assert_eq!(html.matches("<tr class=\"c\">").count(), 1);
        assert_eq!(html.matches("<tr class=\"r\">").count(), 2);
        assert_eq!(html.matches("<tr><td").count(), 1);
        assert_eq!(html.matches("</tr>\n").count(), 6);
        // Column order of the header.
        let h = html.find("<thead>").unwrap();
        let hs = &html[h..html.find("</thead>").unwrap()];
        let order: Vec<usize> = ["USN<", "Drive<", "Name<", "Date<", "Reason<", "Directory<"].iter().map(|t| hs.find(t).unwrap()).collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]));
        assert!(html.contains("<td class=\"usn\">1</td><td class=\"drv\">C</td><td>del</td>"));
    }

    #[test]
    fn html_empty_and_large() {
        let f = TempFile::new("html");
        assert_eq!(write_html(&f.0, &labels(), Vec::new()).unwrap(), 0);
        assert!(f.text().contains("<span class=\"chip\">Rows: <b>0 "));

        let names: Vec<String> = (0..20_000).map(|i| format!("file{i}.dat")).collect();
        let rows: Vec<ExportRow> = names.iter().enumerate().map(|(i, n)| row(i as i64, n, "Data", (i as u32 % 4) << 8)).collect();
        assert_eq!(write_html(&f.0, &labels(), rows).unwrap(), 20_000);
        let html = f.text();
        assert_eq!(html.matches("</tr>\n").count(), 20_000);
        assert!(html.contains("Rows: <b>20000 "));
    }

    #[test]
    fn functions_report_io_errors() {
        let bad = std::env::temp_dir().join("jt_no_such_dir_xyz").join("x.csv");
        assert!(write_csv(&bad, HEADER, Vec::new()).is_err());
        assert!(write_json(&bad, Vec::new()).is_err());
        assert!(write_html(&bad, &labels(), Vec::new()).is_err());
    }
}
