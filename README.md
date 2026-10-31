# JournalTraceRust

A Rust port of [JournalTrace](https://github.com/spokwn/JournalTrace) (spokwn's version of [ponei's original](https://github.com/ponei/JournalTrace)). It parses NTFS USN Journal entries and shows them in a simple user interface.

It is a single `JournalTrace.exe` with no .NET runtime, built with [egui](https://github.com/emilk/egui). Like the original, it must be run as administrator.

## Build

```
cargo build --release
```

The output is `target\release\JournalTrace.exe`. The admin manifest and the icon are embedded in the exe. Embedding the icon needs `rc.exe` from the Windows SDK; without it the build still succeeds, but the exe has no icon.

To try the UI without admin rights or a real journal, run it on generated entries from two drives:

```
cargo run --release --example demo
```

## Features

### Same as the C# version
- Drive selection and journal scan with step-by-step status.
- **Data Grid**: every entry in one table, with a search box, a column picker and reason check boxes.
- **Directory Tree**: a directory tree on the left and the changes in the selected directory on the right.
- Right-click menu: copy the cell, show file information, and open the file's location in Explorer with the file already selected.
- Credits window and background picture.
- Turkish (the default), English and Portuguese.

### New in this version
- **All drives**: scan every NTFS drive into one table, with a Drive column (Drive > Scan all drives).
- **Live monitoring**: new journal records are added to the list every second (Drive > Live monitoring).
- **Timeline** (off by default, Layout > Timeline): a changes-per-hour chart above the grid. Click a bar to filter to that time span; click it again to clear the filter.
- **File history**: the file information window shows the file's rename chain (`a.exe → b.tmp`), its creation and deletion times, and how long it lived.
- **Short-lived filter**: files that were created and deleted within a minute.
- **Search as you type**, with no need to press Enter.
- **Export** to HTML (default), CSV or JSON. The HTML report is a single dark-themed page with filtering and sorting. You can export everything shown or only the selected rows.
- **Multi-row selection**: Ctrl+click and Shift+click. Ctrl+C copies the selected rows as tab-separated text; Ctrl+A selects all; Ctrl+F jumps to the search box.
- **Zoom**: Ctrl +/−, or Ctrl + mouse wheel. Ctrl+0 resets it.
- Directories that no longer exist are rebuilt from the journal's own records (shown in italics).
- Rows are colored by reason, sorted by clicking a column header, and dates sort chronologically.

## Filter syntax

- **Basic search**: `.exe` searches the column picked in the box (default: name).
- **Inclusions**: `name:rundll32&&.pf`
- **Exclusions**: `name:.exe!!svchost`
- **OR conditions**: `name:.exe||.dll`
- **Multiple columns**: `name:.pf;reason:delete;drive:D`
- Columns: `usn`, `name`, `date`, `reason`, `directory`, `drive`. Turkish and Portuguese names also work, e.g. `ad:`, `tarih:`, `neden:`, `dizin:`, `sürücü:`.
- Paths (`C:\Windows\...`) automatically search the directory column.
- **Match mode**: the box next to the column picker chooses *Contains* (default), *Exact match*, *Starts with* or *Ends with*.
- A term with a leading `=` is always an exact match: `name:=svchost.exe`.

Matching is case-insensitive and never uses wildcards: characters like `'`, `[`, `%` and `*` are searched as written.

## Bugs fixed from the C# version
- The reason check boxes stopped working in Portuguese. They now test the reason flag instead of English text.
- Searching for `'` or `[` crashed the program.
- File IDs were resolved against the current directory's volume instead of the scanned one.
- Paths longer than 128 characters were cut off.
- The UI was updated from a background thread.
- 32-bit pointer arithmetic (`IntPtr.ToInt32`).
- File > Export existed but did nothing.

## License
GNU General Public License v3.0, same as the original. See [LICENSE](LICENSE).
