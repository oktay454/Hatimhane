# Hatimhane

Rust ile yazılmış, birden fazla cami ve hatim grubu için sade yönetim ve okuma listesi. Lisans: **GPL-3.0-only**.

- Tek `hatimhane.service`: yönetim paneli, uygulama kimlik doğrulaması, döngü zamanlaması ve JSON yayımlama.
- SQLite3 varsayılan; MariaDB ve PostgreSQL bağlantı URL'siyle seçilebilir.
- Cami adı ve adresi panelden değiştirilir. `/htuc/`, `/htuc/1/`, `/htuc/2/` desteklenir. İşlemler UUID üzerinden yapılır.
- Sunucu sahibi tüm camileri ve kullanıcıları yönetir. Görevli yalnızca kendisine atanan camileri yönetir; kapsam sunucuda denetlenir.
- Her caminin haftalık veya gün aralıklı döngüsü vardır. Grup caminin ayarını kullanabilir veya kendi döngüsünü seçebilir. 30. cüzden sonra 1. cüze dönülür.
- Ziyaretçi istekleri yayımlanmış JSON görüntüsünü(snapshot) okur; veritabanına sorgu göndermez.
- Açık/koyu tema cihaz tercihini izler; kullanıcı değiştirebilir. Büyük yazı ve düğmeler, kapalı başlayan ayrıntılı ayarlar.

## Debian kurulumu

Paket Debian 13 için hazırlanır. Paket dosyasını kurun:

```bash
sudo apt install ./hatimhane_0.2.0_amd64.deb
```

`/etc/hatimhane.d/10-server.toml` içinde `origin` değerini gerçek HTTPS adresinizle değiştirin. Sonunda `/` olmamalı. Örneğin `https://hatim.oktayaktogan.com.tr`.

İlk sunucu sahibi hesabı; parolayı terminalin gizli istemine yazın:

```bash
sudo -u hatimhane hatimhane owner --username yonetici
sudo systemctl restart hatimhane.service
```

Varsayılan hesap veya parola yoktur. Parola en az 12 karakterdir. `/panel/` yönetim panelidir. `packaging/nginx.conf.example` içeriğini HTTPS Nginx sunucunuza uyarlayın. Basic Auth kullanılmaz; uygulama kendi oturumunu yönetir. Nginx `X-Real-IP` başlığını `$remote_addr` ile kendisi yazar. Doğrudan uygulama portunu internete açmayın.

İlk girişten sonra **Yeni cami ekle** → caminin adı → kısa adresi → **Yeni hatim grubu ekle**. İsim düzenlemek için grup → cüz → isim → **İsmi kaydet**. Yenileme ayarları ayrı, kapalı başlayan bölümlerdedir.

Yapılandırma dosyaları ad sırasıyla okunur; sonraki dosyanın aynı anahtarı öncekini geçersiz kılar. Yerel ayarlar için `90-local.toml` kullanılabilir. Veritabanı parolası varsa dosya `root:hatimhane`, izin `0640` olmalı. Veri `/var/lib/hatimhane/` altında kalır; uygulama kaldırılırken otomatik silinmez.

## Eski JSON listesinden geçiş

Eski cron ve iki Python hizmetini kapatın. Önce gerçek, güncel JSON'u ve veritabanını yedekleyin. Güncel JSON'u hizmet kullanıcısının okuyabileceği bir konuma koyun:

```bash
sudo -u hatimhane hatimhane import --json /var/lib/hatimhane/eski-hatim.json --slug htuc
sudo systemctl restart hatimhane.service
```

Eski `cami_adi`, `hafta_baslangici`, `katilimcilar[].ad/cuz` alanları okunur. Cami ve 1 numaralı grup için yeni UUID'ler oluşturulur. Başlangıç döngüsü salı 21.00'dır. Aynı adres ikinci kez içe aktarılırsa çakışma hatası döner; mevcut cami ezilmez. Geçmiş hafta tarihi varsa hizmet açılırken kaçırılan döngüler yakalanır.

Yeni uygulamada cron gerekmez. Önceki Python kurulumunun canlı dosyaları otomatik taşınmaz veya değiştirilmez. Grup adı, cami adı ve diğer yeni ayarlar panelden yönetilir.

## Veritabanı seçimi

İlk kurulumdan önce `database_url` seçin. MariaDB/PostgreSQL için ayrı veritabanı ve yalnızca o veritabanına erişen bir uygulama hesabı oluşturun. Şema ilk bağlantıda hazırlanır.

```toml
# SQLite3
 database_url = "sqlite:///var/lib/hatimhane/hatim.sqlite3?mode=rwc"
# MariaDB
# database_url = "mysql://hatimhane:PAROLA@127.0.0.1/hatimhane"
# PostgreSQL
# database_url = "postgres://hatimhane:PAROLA@127.0.0.1/hatimhane"
```

Paroladaki URL özel karakterlerini yüzde kodlaması(percent encoding) ile yazın. Uzak veritabanı bağlantılarında sertifika doğrulamalı TLS kullanın. URL değiştirmek kayıtları diğer motora taşımaz; bu sürümde motorlar arası otomatik veri taşıma aracı yoktur.

## Güvenlik ve hesap kurtarma

Parolalar Argon2id ile özetlenir(hash). Oturum rastgele 256 bit değerdir; veritabanında yalnızca SHA-256 özeti tutulur. Üretimde çerez(cookie) `__Host-`, `Secure`, `HttpOnly`, `SameSite=Lax` kullanır. Oturum 8 saatte veya 30 dakika hareketsizlikte biter. CSRF belirteci(token) ve tam köken(origin) doğrulaması yazma isteklerinde zorunludur. Giriş denemeleri hesap ve istemci adresi bazında sınırlanır.

**Hesabım** bölümünde parola değişimi ve isteğe bağlı iki aşamalı giriş(TOTP) vardır. Doğrulayıcı anahtarı sunucuda doğrulama için saklanır; veritabanı ve yedekler gizli tutulmalıdır. TOTP giriş kodunun tekrar kullanımına izin verilmez. Hesap/parola/yetki değişikliği ilgili oturumları iptal eder. Son aktif sunucu sahibi hesabı panelden kapatılamaz.

Sunucuya işletim sistemi erişimi olan yönetici hesap kurtarmak için:

```bash
sudo -u hatimhane hatimhane owner --username yonetici --reset
```

Yeni parolayı gizli isteme yazın. Bu işlem iki aşamalı doğrulamayı kaldırır ve hesabın bütün oturumlarını iptal eder. `--password-file` otomasyon/test içindir; parola dosyasını gizli tutun ve işiniz bitince kaldırın. Parola komut satırı argümanına yazılmaz.

## Kaynaktan derleme ve DEB

Debian 13'ün Rust/Cargo 1.85 araçlarıyla uyumlu bağımlılıklar `Cargo.lock` ile sabitlenir. Geliştirme gereksinimleri: Rust/Cargo ≥1.85, C derleyicisi, `pkgconf`, `libsqlite3-dev`; paketleme için `debhelper`, `dpkg-dev`, Python ≥3.11. Eksik paketler otomatik kurulmaz.

```bash
LIBSQLITE3_SYS_USE_PKG_CONFIG=1 cargo build --release --locked
LIBSQLITE3_SYS_USE_PKG_CONFIG=1 cargo test --locked
```

Kaynak DEB hazırlanırken önce bağımlılık kaynaklarını alın; bu adım ağ kullanır:

```bash
packaging/prepare-source.sh
```

Ardından Debian paket derlemesi çevrimdışıdır(offline):

```bash
dpkg-buildpackage -us -uc
```

`vendor/`, `.cargo/config.toml`, derleme çıktıları ve test verileri Git'e eklenmez. Hazırlanmış kaynak paketi, `vendor/` ve üçüncü taraf lisanslarını içerir. Binary DEB sistem SQLite kütüphanesine dinamik bağlanır; bağımlılıklar Debian araçlarıyla hesaplanır. Program `/usr/bin/hatimhane`, hizmet `/usr/lib/systemd/system/hatimhane.service`, yapılandırma `/etc/hatimhane.d/` altına kurulur.

MariaDB/PostgreSQL testi için uygulamadan ayrı bir test veritabanında:

```bash
HATIM_TEST_DATABASE_URL='postgres://...' cargo test --test application external_database_round_trip -- --ignored
```

Normal testler proje içindeki `work/tests/` altında geçici SQLite verisi kullanır. Dış veritabanı testi yalnızca kendi UUID'li camisini/grubunu ve test hesabını temizler. Canlı veritabanında test çalıştırmayın.

## Çalışma ve yedekleme

```bash
systemctl status hatimhane.service
journalctl -u hatimhane.service -n 50 --no-pager
```

JSON dosyaları `/var/lib/hatimhane/public/<CAMI_UUID>.json` ve `index.json` olarak atomik yayımlanır. Asıl kayıtlar veritabanındadır; JSON'a elle yapılan değişiklik sonraki yayında geri alınır. Her caminin JSON'u bütün gruplarını içerir. Adres değişiminde UUID dosyası korunur, adres eşlemesi güncellenir; eski adres kapanır. Cami silinirse eski JSON dosyası temizlenir.

Hizmet zamanı varsayılan 15 saniyede bir kontrol eder; panel kayıtlarından sonra JSON hemen yayımlanır. Ziyaretçi sayfası dakikada bir veya sekmeye dönüldüğünde yenilenir. Yayın başarısızsa kayıt veritabanında korunur, hizmet tekrar dener ve panel aktarımın geciktiğini belirtir. Hizmet kapalıyken kaçırılan döngüler açılışta yakalanır; aynı döngü ikinci kez uygulanmaz.

SQLite yedeği için SQLite backup API, MariaDB için uygun dump, PostgreSQL için pg_dump kullanın. Veritabanını ve yapılandırmayı birlikte yedekleyin; JSON hesap ve yetki verilerini içermediği için tam yedek değildir. Veritabanı büyük şema değişikliklerinden önce ayrıca yedeklenmelidir.

Mimari ayrıntıları: [ARCHITECTURE.md](ARCHITECTURE.md).

Doğrulanan senaryolar ve sınırlar: [TESTING.md](TESTING.md).
