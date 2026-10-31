//! Plain-text search filter for the journal grid.
//!
//! Syntax (superset of the original C# `RowFilter` builder):
//!
//! * `;` separates conditions (all must match).
//! * `column:value` restricts a condition to one column; a bare value uses the
//!   default column (or any column), and path-like bare values go to `Directory`.
//! * Inside a value: `a!!b!!c` (contains `a`, not `b`, not `c`), `a&&b`
//!   (all), `a||b` (any). Priority: `!!`, then `&&`, then `||`.
//!
//! Everything is a case-insensitive substring match; there are no wildcards.
//! When a bare value has no target column (default `None`), the whole value
//! condition is evaluated against each column and the results are OR-ed.

use std::borrow::Cow;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Column {
    Usn,
    Name,
    Date,
    Reason,
    Directory,
    Drive,
}

impl Column {
    pub const ALL: [Column; 6] = [
        Column::Usn,
        Column::Name,
        Column::Date,
        Column::Reason,
        Column::Directory,
        Column::Drive,
    ];

    /// Parse a column name used in `col:value` syntax. Case-insensitive.
    pub fn parse(s: &str) -> Option<Column> {
        let lower: String = s
            .trim()
            .to_lowercase()
            .chars()
            .filter(|&c| c != '\u{307}') // combining dot left by lowercasing 'İ'
            .collect();
        match lower.as_str() {
            "usn" => Some(Column::Usn),
            "name" | "nome" | "ad" | "isim" => Some(Column::Name),
            "date" | "data" | "tarih" => Some(Column::Date),
            "reason" | "razao" | "razão" | "neden" | "sebep" => Some(Column::Reason),
            "directory" | "dir" | "path" | "diretorio" | "diretório" | "dizin" | "klasör"
            | "klasor" => Some(Column::Directory),
            "drive" | "volume" | "disk" | "sürücü" | "surucu" | "unidade" | "disco" => Some(Column::Drive),
            _ => None,
        }
    }
}

pub trait FilterRow {
    fn field(&self, col: Column) -> Cow<'_, str>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Col(Column),
    Any,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    All,
    Any,
}

#[derive(Clone, Debug)]
struct Term {
    /// Lowercased needle.
    needle: String,
    ascii: bool,
    negate: bool,
}

#[derive(Clone, Debug)]
struct Cond {
    target: Target,
    mode: Mode,
    terms: Vec<Term>,
}

#[derive(Clone, Debug, Default)]
pub struct Filter {
    conds: Vec<Cond>,
}

fn is_path(s: &str) -> bool {
    !s.trim().is_empty()
        && (s.contains(":\\")
            || s.starts_with("\\\\")
            || s.starts_with('/')
            || s.starts_with("./")
            || s.starts_with("../"))
}

/// Lowercase and fold the Turkish dotted/dotless i, so I, dotless i, dotted I and i all compare equal.
fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.to_lowercase().chars() {
        match c {
            '\u{307}' => {}
            'ı' => out.push('i'),
            c => out.push(c),
        }
    }
    out
}

fn term(part: &str, negate: bool) -> Term {
    let needle = fold(part.trim());
    let ascii = needle.is_ascii();
    Term {
        needle,
        ascii,
        negate,
    }
}

/// Build the (mode, terms) of a value, or None when it has no usable parts.
fn parse_value(value: &str) -> Option<(Mode, Vec<Term>)> {
    let value = value.trim();
    let nonempty = |p: &&str| !p.trim().is_empty();
    let (mode, terms): (Mode, Vec<Term>) = if value.contains("!!") {
        let terms = value
            .split("!!")
            .enumerate()
            .filter(|(_, p)| nonempty(p))
            .map(|(i, p)| term(p, i != 0))
            .collect();
        (Mode::All, terms)
    } else if value.contains("&&") {
        (
            Mode::All,
            value
                .split("&&")
                .filter(nonempty)
                .map(|p| term(p, false))
                .collect(),
        )
    } else if value.contains("||") {
        (
            Mode::Any,
            value
                .split("||")
                .filter(nonempty)
                .map(|p| term(p, false))
                .collect(),
        )
    } else {
        (Mode::All, vec![term(value, false)])
    };
    let terms: Vec<Term> = terms.into_iter().filter(|t| !t.needle.is_empty()).collect();
    if terms.is_empty() {
        None
    } else {
        Some((mode, terms))
    }
}

/// `needle` must already be lowercase ASCII.
fn ascii_contains(hay: &[u8], needle: &[u8]) -> bool {
    let n = needle.len();
    if n == 0 {
        return true;
    }
    if hay.len() < n {
        return false;
    }
    let first = needle[0];
    let last_start = hay.len() - n;
    let mut i = 0;
    while i <= last_start {
        if hay[i].to_ascii_lowercase() == first
            && hay[i + 1..i + n]
                .iter()
                .zip(&needle[1..])
                .all(|(h, nb)| h.to_ascii_lowercase() == *nb)
        {
            return true;
        }
        i += 1;
    }
    false
}

impl Cond {
    fn eval(&self, hay: &str) -> bool {
        if hay.is_ascii() {
            // Lowercasing ASCII yields ASCII, so a non-ASCII needle cannot match.
            self.combine(|t| t.ascii && ascii_contains(hay.as_bytes(), t.needle.as_bytes()))
        } else {
            let lowered = fold(hay);
            self.combine(|t| lowered.contains(t.needle.as_str()))
        }
    }

    fn combine(&self, contains: impl Fn(&Term) -> bool) -> bool {
        let check = |t: &Term| contains(t) != t.negate;
        match self.mode {
            Mode::All => self.terms.iter().all(check),
            Mode::Any => self.terms.iter().any(check),
        }
    }

    fn eval_row(&self, row: &impl FilterRow) -> bool {
        match self.target {
            Target::Col(c) => self.eval(&row.field(c)),
            Target::Any => Column::ALL.iter().any(|&c| self.eval(&row.field(c))),
        }
    }
}

fn bare_target(value: &str, default_column: Option<Column>) -> Target {
    if is_path(value) {
        Target::Col(Column::Directory)
    } else {
        match default_column {
            Some(c) => Target::Col(c),
            None => Target::Any,
        }
    }
}

impl Filter {
    /// `default_column`: column used for bare values (no `col:` prefix).
    /// `None` = match if ANY column matches.
    pub fn parse(text: &str, default_column: Option<Column>) -> Filter {
        let mut conds = Vec::new();
        for seg in text.trim().split(';') {
            let seg = seg.trim();
            if seg.is_empty() {
                continue;
            }
            let (target, value) = match seg.split_once(':') {
                Some((word, rest)) => match Column::parse(word) {
                    Some(c) => (Target::Col(c), rest),
                    None => (bare_target(seg, default_column), seg),
                },
                None => (bare_target(seg, default_column), seg),
            };
            if let Some((mode, terms)) = parse_value(value) {
                conds.push(Cond {
                    target,
                    mode,
                    terms,
                });
            }
        }
        Filter { conds }
    }

    /// True when the text produced no conditions.
    pub fn is_empty(&self) -> bool {
        self.conds.is_empty()
    }

    /// True if evaluating may read this column.
    pub fn uses_column(&self, col: Column) -> bool {
        self.conds.iter().any(|c| match c.target {
            Target::Col(x) => x == col,
            Target::Any => true,
        })
    }

    /// An empty filter matches everything.
    pub fn matches(&self, row: &impl FilterRow) -> bool {
        self.conds.iter().all(|c| c.eval_row(row))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Row {
        usn: &'static str,
        name: &'static str,
        date: &'static str,
        reason: &'static str,
        dir: &'static str,
    }

    impl FilterRow for Row {
        fn field(&self, col: Column) -> Cow<'_, str> {
            Cow::Borrowed(match col {
                Column::Usn => self.usn,
                Column::Name => self.name,
                Column::Date => self.date,
                Column::Reason => self.reason,
                Column::Directory => self.dir,
                Column::Drive => "C",
            })
        }
    }

    fn row(name: &'static str, reason: &'static str, dir: &'static str) -> Row {
        Row {
            usn: "12345",
            name,
            date: "2024-01-02 10:00:00",
            reason,
            dir,
        }
    }

    fn m(text: &str, def: Option<Column>, r: &Row) -> bool {
        Filter::parse(text, def).matches(r)
    }

    #[test]
    fn empty_input() {
        let r = row("a.exe", "File create", "C:\\x");
        for t in ["", "   ", ";;", " ; ; ", "name:", "name:  ", "name:!!", "&&", "||"] {
            let f = Filter::parse(t, None);
            assert!(f.is_empty(), "{t:?}");
            assert!(f.matches(&r));
            assert!(!f.uses_column(Column::Name));
        }
    }

    #[test]
    fn readme_bare_extension() {
        let r = row("notepad.exe", "Close", "C:\\Windows");
        assert!(m(".exe", Some(Column::Name), &r));
        assert!(!m(".dll", Some(Column::Name), &r));
        assert!(m(".exe", None, &r));
    }

    #[test]
    fn readme_and() {
        let f = Filter::parse("name:rundll32&&.pf", None);
        assert!(f.matches(&row("RUNDLL32.EXE-ABC.pf", "", "")));
        assert!(!f.matches(&row("rundll32.exe", "", "")));
        assert!(!f.matches(&row("other.pf", "", "")));
    }

    #[test]
    fn readme_not() {
        let f = Filter::parse("name:.exe!!svchost", None);
        assert!(f.matches(&row("a.exe", "", "")));
        assert!(!f.matches(&row("svchost.exe", "", "")));
        assert!(!f.matches(&row("a.dll", "", "")));
    }

    #[test]
    fn not_with_empty_first_part() {
        let f = Filter::parse("name:!!svchost", None);
        assert!(!f.is_empty());
        assert!(f.matches(&row("a.exe", "", "")));
        assert!(!f.matches(&row("SvcHost.exe", "", "")));
        let f = Filter::parse("name:!!a!!b", None);
        assert!(f.matches(&row("c", "", "")));
        assert!(!f.matches(&row("xbx", "", "")));
        assert!(!f.matches(&row("xax", "", "")));
    }

    #[test]
    fn readme_or() {
        let f = Filter::parse("name:.exe||.dll", None);
        assert!(f.matches(&row("a.exe", "", "")));
        assert!(f.matches(&row("a.dll", "", "")));
        assert!(!f.matches(&row("a.txt", "", "")));
        // empty alternatives are ignored
        let f = Filter::parse("name:||.dll||", None);
        assert!(f.matches(&row("a.dll", "", "")));
        assert!(!f.matches(&row("a.txt", "", "")));
    }

    #[test]
    fn readme_multi_condition() {
        let f = Filter::parse("name:.pf;reason:delete", None);
        assert!(f.matches(&row("a.pf", "File delete", "")));
        assert!(!f.matches(&row("a.pf", "File create", "")));
        assert!(!f.matches(&row("a.exe", "File delete", "")));
        assert!(f.uses_column(Column::Name));
        assert!(f.uses_column(Column::Reason));
        assert!(!f.uses_column(Column::Directory));
    }

    #[test]
    fn paths_go_to_directory() {
        let r = row("a.exe", "", "C:\\Windows\\System32");
        let other = row("a.exe", "", "D:\\Data");
        for def in [None, Some(Column::Name), Some(Column::Reason)] {
            assert!(m("C:\\Windows\\System32", def, &r));
            assert!(m("c:\\windows", def, &r));
            assert!(!m("C:\\Windows", def, &other));
        }
        assert!(m("directory:C:\\Windows", None, &r));
        assert!(!m("directory:C:\\Windows", None, &other));
        assert!(m("dir:c:\\windows;name:.exe", None, &r));
        let f = Filter::parse("C:\\Windows", Some(Column::Name));
        assert!(f.uses_column(Column::Directory));
        assert!(!f.uses_column(Column::Name));
    }

    #[test]
    fn path_prefixes() {
        for p in ["\\\\server\\share", "/usr", "./x", "../x"] {
            let f = Filter::parse(p, Some(Column::Name));
            assert!(f.uses_column(Column::Directory), "{p}");
            assert!(!f.uses_column(Column::Name), "{p}");
        }
        let f = Filter::parse("abc", Some(Column::Name));
        assert!(!f.uses_column(Column::Directory));
    }

    #[test]
    fn special_characters_are_literal() {
        assert!(m("O'Brien", None, &row("O'Brien.txt", "", "")));
        assert!(!m("O'Brien", None, &row("OBrien.txt", "", "")));
        assert!(m("[1]", None, &row("file[1].txt", "", "")));
        assert!(!m("[1]", None, &row("file1.txt", "", "")));
        assert!(m("100%", None, &row("100%.txt", "", "")));
        assert!(!m("100%", None, &row("1000.txt", "", "")));
        assert!(!m("*", None, &row("abc", "", "")));
        assert!(m("*", None, &row("a*c", "", "")));
        assert!(!m("a%c", None, &row("abc", "", "")));
        assert!(m("name:it's", None, &row("it's", "", "")));
    }

    #[test]
    fn case_insensitive() {
        assert!(m("NAME:NOTEPAD", None, &row("notepad.exe", "", "")));
        assert!(m("notepad", None, &row("NotePad.EXE", "", "")));
        // non-ASCII haystack
        assert!(m("name:çağrı", None, &row("ÇAĞRI.txt", "", "")));
        assert!(m("name:ÇAĞRI", None, &row("çağrı.txt", "", "")));
        assert!(m("name:çağri", None, &row("ÇAĞRI.txt", "", "")));
        assert!(m("name:çağrı", None, &row("ÇAĞRİ.txt", "", "")));
        assert!(m("name:kım", None, &row("kim.txt", "", "")));
        assert!(m("name:txt", None, &row("ÇAĞRI.TXT", "", "")));
        assert!(m("name:ação", None, &row("AÇÃO.doc", "", "")));
        // non-ASCII needle never matches an ASCII haystack
        assert!(!m("name:ç", None, &row("c.txt", "", "")));
    }

    #[test]
    fn unknown_column_is_bare_value() {
        let f = Filter::parse("foo:bar", None);
        assert!(!f.is_empty());
        assert!(f.matches(&row("xfoo:barx", "", "")));
        assert!(!f.matches(&row("bar", "", "")));
        let f = Filter::parse("foo:bar", Some(Column::Reason));
        assert!(f.uses_column(Column::Reason));
        assert!(!f.uses_column(Column::Name));
        assert!(f.matches(&row("", "foo:bar", "")));
    }

    #[test]
    fn default_column_none_vs_some() {
        let r = row("alpha", "File delete", "C:\\Dir");
        assert!(m("delete", None, &r));
        assert!(!m("delete", Some(Column::Name), &r));
        assert!(m("delete", Some(Column::Reason), &r));
        assert!(m("12345", None, &r));
        assert!(m("2024-01", None, &r));
        let f = Filter::parse("x", None);
        for c in Column::ALL {
            assert!(f.uses_column(c));
        }
        let f = Filter::parse("x", Some(Column::Date));
        assert!(f.uses_column(Column::Date));
        assert!(!f.uses_column(Column::Usn));
    }

    #[test]
    fn any_column_with_operators() {
        let r = row("alpha", "File delete", "");
        // the whole condition must hold within ONE column
        assert!(!m("alpha&&delete", None, &r));
        assert!(m("alp||zzz", None, &r));
        assert!(m("file!!create", None, &r));
        assert!(!m("file!!delete", None, &r));
    }

    #[test]
    fn column_aliases() {
        assert_eq!(Column::parse("USN"), Some(Column::Usn));
        assert_eq!(Column::parse(" Name "), Some(Column::Name));
        for s in ["nome", "ad", "isim", "İsim"] {
            assert_eq!(Column::parse(s), Some(Column::Name), "{s}");
        }
        for s in ["data", "tarih", "DATE"] {
            assert_eq!(Column::parse(s), Some(Column::Date), "{s}");
        }
        for s in ["razao", "razão", "neden", "sebep", "Reason"] {
            assert_eq!(Column::parse(s), Some(Column::Reason), "{s}");
        }
        for s in [
            "dir",
            "path",
            "diretorio",
            "diretório",
            "dizin",
            "klasör",
            "klasor",
            "Directory",
        ] {
            assert_eq!(Column::parse(s), Some(Column::Directory), "{s}");
        }
        for s in ["", "c", "foo", "d", "http"] {
            assert_eq!(Column::parse(s), None, "{s}");
        }
        assert!(m("tarih:2024", None, &row("", "", "")));
        assert!(m("razão:delete", None, &row("", "File delete", "")));
    }

    #[test]
    fn windows_drive_letter_is_not_a_column() {
        let r = row("a", "", "C:\\Windows");
        assert!(m("C:\\Windows", None, &r));
        assert!(!m("D:\\Windows", None, &r));
    }

    #[test]
    fn whitespace_is_trimmed() {
        let r = row("a.exe", "", "");
        assert!(m("  name :  .exe  ", None, &r));
        assert!(m("name: a && .exe ", None, &r));
        assert!(m(" ; name:.exe ; ", None, &r));
    }

    #[test]
    fn no_panics_on_odd_input() {
        let r = row("a", "b", "c");
        for t in [
            ":", "::", ";:;", "name::", "!!", "&&||!!", "name:!!&&||", "\u{307}:x", "💥:💥",
            "name:💥", "\\", ":\\", "ß:ß", "İ", "name:İ",
        ] {
            let _ = Filter::parse(t, None).matches(&r);
            let _ = Filter::parse(t, Some(Column::Name)).matches(&r);
        }
    }

    #[test]
    fn ascii_search_helper() {
        assert!(ascii_contains(b"Hello World", b"o w"));
        assert!(ascii_contains(b"abc", b"abc"));
        assert!(ascii_contains(b"abc", b""));
        assert!(!ascii_contains(b"ab", b"abc"));
        assert!(!ascii_contains(b"abcd", b"bd"));
        assert!(ascii_contains(b"xxABC", b"abc"));
    }

    #[test]
    fn performance_sanity() {
        let rows: Vec<Row> = (0..20_000)
            .map(|_| {
                row(
                    "somefile_with_long_name.dll",
                    "File create",
                    "C:\\Windows\\System32\\drivers",
                )
            })
            .collect();
        let f = Filter::parse("name:.exe||.sys;dir:windows", None);
        assert_eq!(rows.iter().filter(|r| f.matches(*r)).count(), 0);
    }
}
