# Doğrulama — 5 Ekim 2026

## Uygulama

4 birim testi(unit test) ve 7 bütünleşme testi(integration test) geçti. Harici veritabanı testi normal çalıştırmada bilerek atlanır; aşağıdaki iki ayrı motorda ayrıca çalıştırıldı.

- SQLite3: hesap/parola, URI doğrulaması, haftalık ve gün aralıklı döngü, 30 → 1 geçişi, kaçırılmış döngüleri yakalama, tekrar çalıştırma, eski grup sürümüyle kayıt reddi.
- Yetkilendirme: görevli başka caminin hiçbir yönetim işlemine erişemiyor; sunucu sahibi bütün camilere erişiyor. URI çakışması reddediliyor, ad/adres değişikliğinde UUID korunuyor.
- Kimlik doğrulama: CSRF ve köken(origin) denetimi, oturum iptali, son aktif sahibin korunması, TOTP kurulum doğrulaması, aynı giriş kodunun tekrar reddi, giriş denemesi sınırı.
- Yayımlama: ziyaretçi veritabanı bağlantısı kapalıyken yayımlanmış veriyi okuyabiliyor; JSON hesap/parola/oturum içermiyor. Değişiklik yoksa dosya yeniden yazılmıyor.

Debian 13.6 SSH test ortamında **MariaDB 11.8.6** ve **PostgreSQL 17.11** ile harici veritabanı testi geçti. Her iki motorda cami/grup oluşturma, Türkçe isim kaydı, görevli yetkisi, giriş, üç döngüyü yakalama ve idempotent yeniden çalıştırma doğrulandı. MariaDB denemesi, veritabanının ikili varsayılan karşılaştırması(collation) altında da tablo metin alanlarının doğru okunmasını kapsadı.

## Tarayıcı

Yönetim panelinde giriş, ikinci grup oluşturma, cüz seçerek isim değiştirme ve üç günlük bağımsız grup döngüsü denendi. Ziyaretçi ekranında grup bağlantısı ve cüz numarasına göre artan liste doğrulandı. 375 × 812 mobil görünümde ve koyu temada yatay taşma görülmedi. Kodlar harici JavaScript/CSS dosyalarındadır; uygulama CSP başlığı satır içi betiğe izin vermez. Tarayıcı eklentilerinin kendi betiklerini enjekte etmesi uygulamanın sorumluluğunda değildir.

## Debian paketi

`dpkg-buildpackage` ile amd64 ikili ve `3.0 (native)` kaynak paketi üretildi. Gerçek derleme ve normal testler çevrimdışı bağımlılık kaynakları(vendor) üzerinden geçti. Sistem SQLite kütüphanesine dinamik bağlantı ve otomatik DEB bağımlılıkları denetlendi.

Hazırlanmış kaynak arşivi ayrıca Debian 13'ün **Rust/Cargo 1.85.0** araçlarıyla `cargo check --offline --locked` denetiminden geçti. Cargo denetim toplamları(checksum) için gereken bağımlılık dosyaları, özellikle ring'in hazır `.o` dosyaları, kaynak paketinde korunur.

DEB, SSH test makinesinde ayrı geçici dizine açıldı. İçindeki çalıştırılabilir dosya, paket birim dosyasından türetilmiş ayrı bir SystemD test hizmetinde çalıştırıldı. Uygulama yolu/veri dizini ve test kullanıcısı uyarlanırken hizmet korumaları korundu. Tek süreçte giriş, CSRF korumalı isim kaydı, anında JSON yayını ve zamanlayıcının 30 → 1 geçişi geçti. Canlı kurulumun dosyaları değiştirilmedi; sistem genelinde DEB kurulumu yapılmadı.

Lintian denetimi tamamlandı. Belirli açıklanmış istisnalar: parola içerebilen yapılandırmanın 0640 izni; Cargo'nun özgün bildirim dosyaları; Debian 13 denetleyicisinin henüz tanımadığı yeni Debian Policy sürümü. Test veritabanları ve test hizmeti doğrulamadan sonra durduruldu.

## Kapsam

Üretim alan adına kurulum, gerçek TLS sertifikası, canlı Nginx yapılandırması ve mevcut kullanıcı verisinin taşınması bu testlerin kapsamına girmedi. Yapılandırma URL'sini değiştirmek veritabanları arasında veri taşımaz. Bir uygulama örneği(instance) desteklenir.
