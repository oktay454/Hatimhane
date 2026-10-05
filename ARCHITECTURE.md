# Hatimhane mimarisi

## Tek hizmet ve yayımlanmış JSON

`hatimhane.service` tek Rust sürecini çalıştırır. Aynı süreç üç işi üstlenir: yönetim HTTP arayüzü(API), döngü zamanlayıcısı(scheduler), JSON yayıncısı(publisher). Cron, ayrı JSON hizmeti veya salt okunur ikinci hizmet gerekmez.

Ziyaretçi tarafı canlı veritabanı sorgusu kullanmaz. Cami/grup listeleri bellekteki yayımlanmış görüntüden(snapshot) okunur; görüntünün kalıcı kopyası cami UUID'sine göre ayrı JSON dosyasıdır. Böylece ziyaretçi istekleri kullanıcı, parola ve oturum tablolarını sorgulayamaz. Bir JSON çıktısı tek başına bütün uygulama için güvenlik garantisi değildir; panel güvenliği ayrıca kimlik doğrulama, yetkilendirme ve istek doğrulamasıyla sağlanır.

| Yol | Görev |
| --- | --- |
| `/` | Cami seçimi |
| `/<cami-uri>/` | Gruplar; tek grup varsa doğrudan cüz listesi |
| `/<cami-uri>/<grup-numarası>/` | Seçilen grup |
| `/veri/<cami-uri>.json` | Yayımlanmış cami ve grup verisi |
| `/panel/` | Giriş ve yönetim |
| `/api/...` | Kimliği doğrulanmış panel işlemleri |

Cami URI'si 2–64 küçük ASCII harf, rakam veya kısa çizgidir; yol ve özel uygulama adları kabul edilmez. Benzersizlik veritabanı UNIQUE kısıtı ile sunucuda garanti edilir. Grup numarası cami içinde benzersizdir. Yeniden adlandırma hiçbir kimliği değiştirmez. Eski adres için takma ad(alias) veya yönlendirme tutulmaz.

## Kayıtlar ve yetkiler

`mosques`, `hatim_groups`, `readers`, `users` UUID anahtarlarla tutulur. UUID'ler üç veritabanında da kanonik `VARCHAR(36)` temsili kullanır. URI ve grup numarası yalnızca ziyaretçi adreslemesidir. İsim değişikliği, okuyucu UUID'si ve grup sürümü(version) ile yapılır. Döngüde okuyucunun UUID'si ve ismi korunur, cüz numarası değişir.

`permissions` hesabı camilere bağlar. `owner` tüm camiler ve hesaplar üzerinde yetkilidir; `manager` yalnızca atanan camilerde isim, grup, adres ve döngü ayarlarını yönetir. Yeni cami oluşturma/silme ve görevli atama sunucu sahibine aittir. Sadece panelde düğme gizlemek yetkilendirme sayılmaz; her API isteği kapsamı sunucuda kontrol eder. Görevli başka cami UUID'si göndererek kapsamı genişletemez.

## Döngü

Cami düzeyindeki ayar gruplara varsayılan olarak geçer. Grup `schedule=null` ile cami ayarını izler veya kendi ayarını saklar. Haftalık gün/saat ya da 1–365 gün aralığı ve saat seçilebilir. Saat dilimi yapılandırmada varsayılan `Europe/Istanbul`.

Yeni grupta ilk yenileme seçilen saatin sonraki gelişidir; gün aralığı ilk yenilemeden itibaren uygulanır. Haftalık seçenekte seçilen gün/saatin sonraki gelişi kullanılır. Ayar değişikliği bir sonraki tarihi yeniden belirler; anında cüz ilerletmez. Sonraki tarih panelde ve ziyaretçi listesinde görünür.

Zamanlayıcı `next_due` sınırına ulaşan grubun kaç döngü kaçırdığını hesaplar. `new_juz=(old_juz-1+count)%30+1`. Sürüm güncellemesi ve bütün okuyucu cüzlerinin değiştirilmesi tek veritabanı işlemi(transaction) içindedir. 30 kişi zorunlu değildir; mevcut cüzler yine 1–30 arasında ilerler. Yeni grupta düzenlenebilir 30 cüz yuvası açılır.

Yazmalar süreç içindeki kilitle(lock) sıralanır. İsim kaydı grup sürümünü şart koşar: diğer yönetici veya zamanlayıcı araya girerse eski ekranın kaydı 409 ile reddedilir. UUID aynı kalsa da yanlış haftadaki cüz adına kayıt yapılamaz. Döngü güncellemesi de sürüm koşuluyla tekrar çalıştırmaya dayanıklıdır(idempotent).

## Kimlik doğrulama

Argon2id: 19 MiB bellek, iki geçiş, tek paralellik. Oturum değeri kriptografik rastgele 256 bittir; yalnızca SHA-256 özeti veritabanında tutulur. CSRF belirteci oturuma bağlıdır; yazma işlemleri ayrıca tam HTTPS origin ve özel panel başlığı ister. Güvenli çerez JavaScript tarafından okunamaz. Sabit/boş varsayılan parola yoktur.

Oturum 8 saat mutlak, 30 dakika hareketsizlik sınırı kullanır. Giriş denemeleri hesap ve istemci adresi için 15 dakikalık pencerede sınırlandırılır; hız sınırı(rate limit) süreç belleğindedir ve hizmet yeniden başlatıldığında sıfırlanır. Nginx gerçek istemci adresi başlığını kendisi yazmalıdır; yalnızca yerel geri döngü(loopback) proxy güvenilir sayılır.

İsteğe bağlı TOTP altı haneli/30 saniyelik kod kullanır; başarılı giriş kodunun zaman adımı tekrar kabul edilmez. Kurulum doğrulanmadan etkinleşmez. Mevcut parola ile yeniden doğrulama(reauthentication) parola ve TOTP değişikliğinde gerekir. TOTP sırrı doğrulama için geri okunabilir saklanır; işletim sistemi izinleri ve gizli yedekler bu nedenle önemlidir. İşletim sistemi erişimli hesap kurtarma bütün oturumları iptal eder.

## Veritabanı taşınabilirliği

SQLx çalışma zamanında SQLite3, MySQL/MariaDB veya PostgreSQL sürücüsünü seçer. Parametreli sorgular(prepared statements) ve motorlara göre yer tutucu(placeholder) uyarlaması kullanılır. Ortak şema UUID, metin ve BIGINT üzerine kuruludur. `schema_version` desteklenmeyen sürümü reddeder; gelecekteki şema değişiklikleri numaralı göç(migration) gerektirir.

SQLite tek bağlantı ve foreign key denetimi kullanır. MariaDB/PostgreSQL için beş bağlantılı havuz(pool) vardır. Bir Hatimhane örneği ve bir asıl veritabanı desteklenir; çoklu uygulama kopyasında dağıtık zamanlayıcı liderliği bu sürümün kapsamı değildir. Veritabanı motoru seçilebilir, ancak URL değişikliği otomatik veri dönüşümü yapmaz.

## Debian yerleşimi

- `/usr/bin/hatimhane`: çalıştırılabilir dosya.
- `/usr/lib/systemd/system/hatimhane.service`: tek hizmet.
- `/etc/hatimhane.d/*.toml`: sırayla birleştirilen yapılandırma, ana dosya conffile.
- `/var/lib/hatimhane/`: uygulama verisi ve yayımlanmış JSON.
- `/usr/share/doc/hatimhane/`: belgeler ve bağımlılık lisansları.

Hizmet ayrı sistem kullanıcısıyla çalışır; korumalar yazmayı uygulama veri diziniyle sınırlar. Paket kaldırma kullanıcı verisini otomatik silmez. Kaynak paket bağımlılık kaynaklarını(vendor) taşır; Debian derlemesi ağ kullanmaz. SQLite sistem kütüphanesine bağlanır ve DEB bağımlılıkları `dpkg-shlibdeps` ile üretilir.
