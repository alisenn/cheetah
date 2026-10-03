# wolf

Çok az RAM kullanan, hafif, açık kaynak masaüstü tarayıcı. Windows, macOS ve Linux'ta çalışır.

Kendi render motorunu yazmaz; [`tao`](https://github.com/tauri-apps/tao) (pencere) ve
[`wry`](https://github.com/tauri-apps/wry) (sistemin kendi WebView'ı) kullanır. Tek process, tek pencere:
üstte araç çubuğu webview'ı (sekme şeridi + adres çubuğu), altında sekme başına bir webview.

## Özellikler

- Sekme açma, kapatma, geçiş (son sekme kapatılamaz)
- Akıllı adres çubuğu: URL ise açar, değilse DuckDuckGo'da arar
- Geri, ileri, yenile
- Sekme başlığı ve URL'i sayfadan canlı güncellenir
- RAM tasarrufu: 120 sn boşta kalan arka plan sekmelerinin webview'ı yok edilir (URL ve başlık saklanır),
  sekmeye geçince yeniden oluşturulur. Uyuyan sekmeler italik ve soluk görünür
- Pencere boyutu değişince tüm webview'lar yeniden boyutlanır
- Bellek göstergesi: adres çubuğunun sağında wolf + webview süreçlerinin toplam RAM'i ve uyanık/toplam sekme sayısı (5 sn'de bir güncellenir)
- Koyu tema

## Çalıştırma

```bash
cargo run --release
```

**Linux:** `libwebkit2gtk-4.1-dev` ve `libgtk-3-dev` kurulu olmalı. `build_as_child` Wayland'de çalışmayabilir:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev
GDK_BACKEND=x11 cargo run --release
```

## Yol haritası

- [ ] Sekme başına bellek ve "bu sekmeyi uyutma" (sabitleme)
- [ ] Toplam bellek bütçesi (aşınca en eski sekmeleri uyut)
- [ ] Kısayollar (Ctrl/Cmd+T, W, L, R)
- [ ] Yer imleri ve geçmiş
- [ ] Sekme sürükle-bırak sıralama
- [ ] Ayarlanabilir arama motoru ve uyku süresi
- [ ] Sayfa içi arama, indirme yönetimi
- [ ] Oturumu geri yükleme

## Lisans

MIT
