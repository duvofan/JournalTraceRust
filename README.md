# JournalTrace (Rust)

**English** | [Türkçe](README.tr.md)

Shows **when files were created, deleted or renamed** on Windows. It reads this from NTFS's own change log (the USN Journal), so a record stays even after the file is deleted.

## Download
Get **JournalTrace.exe** from the [Releases](https://github.com/duvofan/JournalTraceRust/releases) page. There is nothing to install; it's a single file. It asks for administrator rights when it opens.

## How to use
1. **Drive → Scan** (or **Scan all drives**).
2. Type what you're looking for in the box at the top. The list filters as you type.
3. **Right-click** a row to see the file's full history or show it in Explorer.

If the journal was deleted and recreated recently, a **red warning** appears at the top. This can mean someone tried to clear their tracks.

## Search examples
| Type | Finds |
|---|---|
| `.exe` | Everything with `.exe` in its name |
| `name:.pf;reason:delete` | Deleted Prefetch (`.pf`) files |
| `name:.exe!!svchost` | `.exe` files that aren't `svchost` |
| `name:.exe\|\|.dll` | `.exe` or `.dll` |
| `name:=svchost.exe` | Files named exactly `svchost.exe` |
| `drive:D;reason:create` | Files created on drive D: |

Next to the search box you can also choose how to match: **Contains**, **Exact match**, **Starts with** or **Ends with**.

## Features
- One drive or all drives, in a single table
- Live monitoring: new changes appear in the list right away
- File history: renames (`a.exe → b.tmp`) and how long the file existed
- "Short-lived" filter: files created and deleted within a minute
- Export to HTML, CSV or JSON
- Ctrl+C, Ctrl+A, Ctrl+F shortcuts; Ctrl + mouse wheel to zoom
- English, Türkçe, Português. It opens in your Windows language.

## Building from source
You need [Rust](https://rustup.rs):
```
cargo build --release
```
The exe ends up in `target\release\JournalTrace.exe`. To try it with sample data and no administrator rights: `cargo run --release --example demo`

## Credits
- [spokwn/JournalTrace](https://github.com/spokwn/JournalTrace): version 1.2 and the advanced filters
- [ponei/JournalTrace](https://github.com/ponei/JournalTrace): the original program
- nolanblew and StCroixSkipper: Win32 API wrappers

## License
GPL-3.0, same as the original project. See [LICENSE](LICENSE).
