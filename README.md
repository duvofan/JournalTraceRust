# JournalTrace (Rust)

Windows'ta hangi dosyanın **ne zaman oluşturulduğunu, silindiğini ya da adının değiştirildiğini** gösterir. Bunu NTFS'in kendi değişiklik kaydından (USN Journal) okur. Dosya silinmiş olsa bile kaydı durur.

## İndir
[Releases](https://github.com/duvofan/JournalTraceRust/releases) sayfasından **JournalTrace.exe**'yi indir. Kurulum yok, tek dosya. Açılırken yönetici izni ister.

## Kullanım
1. **Sürücü → Tara** (ya da **Tüm sürücüleri tara**).
2. Üstteki kutuya aradığını yaz; liste yazdıkça süzülür.
3. Bir satıra **sağ tıkla**: dosyanın tüm geçmişini gör ya da Explorer'da göster.

Günlük yakın zamanda silinip yeniden oluşturulmuşsa üstte **kırmızı bir uyarı** çıkar. Bu, birinin izlerini temizlemeye çalıştığını gösterebilir.

## Arama örnekleri
| Yaz | Bulur |
|---|---|
| `.exe` | Adında `.exe` geçen her şey |
| `ad:.pf;neden:silme` | Silinen Prefetch (`.pf`) dosyaları |
| `ad:.exe!!svchost` | `.exe` olup `svchost` olmayanlar |
| `ad:.exe\|\|.dll` | `.exe` ya da `.dll` |
| `ad:=svchost.exe` | Adı tam olarak `svchost.exe` olanlar |
| `sürücü:D;neden:oluşturma` | D: sürücüsünde oluşturulan dosyalar |

Arama kutusunun yanından eşleşme türünü de seçebilirsin: **İçerir**, **Tam eşleşme**, **İle başlar**, **İle biter**.

## Özellikler
- Tek sürücü ya da tüm sürücüler, tek tabloda
- Canlı izleme: yeni değişiklikler anında listeye düşer
- Dosya geçmişi: ad değişiklikleri (`a.exe → b.tmp`) ve dosyanın ne kadar yaşadığı
- "Kısa ömürlü" filtresi: 1 dakika içinde oluşturulup silinen dosyalar
- HTML, CSV ya da JSON olarak dışa aktarma
- Ctrl+C, Ctrl+A, Ctrl+F kısayolları; Ctrl + fare tekerleği ile yakınlaştırma
- Türkçe, English, Português. Windows dili neyse onunla açılır.

## Kaynaktan derleme
[Rust](https://rustup.rs) kurulu olmalı:
```
cargo build --release
```
Exe `target\release\JournalTrace.exe` konumuna çıkar. Yönetici izni olmadan, sahte veriyle denemek için: `cargo run --release --example demo`

## Teşekkürler
- [spokwn/JournalTrace](https://github.com/spokwn/JournalTrace): 1.2 sürümü ve gelişmiş filtreler
- [ponei/JournalTrace](https://github.com/ponei/JournalTrace): orijinal program
- nolanblew ve StCroixSkipper: Win32 API sarmalayıcıları

## English
JournalTrace reads the NTFS USN Journal and shows when files were created, deleted or renamed. Download **JournalTrace.exe** from Releases and run it as administrator. Then use Drive → Scan. It warns you when the journal was recreated recently, which can mean someone cleared it.

## Lisans
GPL-3.0, orijinal projeyle aynı. Ayrıntılar: [LICENSE](LICENSE).
