"use strict";
(() => {
	const bul = id => document.getElementById(id);
	let session = null, mosques = [], activeMosque = null, activeGroup = null, dirty = false, busy = false;
	const view = bul("gorunum");
	const text = (tag, value, cls = "") => { const e = document.createElement(tag); e.textContent = value; if (cls) e.className = cls; return e; };
	const button = (value, action, cls = "") => { const e = text("button", value, cls); e.type = "button"; e.addEventListener("click", action); return e; };
	function status(value, error = false) { bul("durum").textContent = value; bul("durum").classList.toggle("hata", error); }
	function title(value, mosque = "") { bul("baslik").textContent = value; bul("cami-adi").textContent = mosque; bul("cami-adi").hidden = !mosque; document.title = `${mosque || value} | Hatimhane`; }
	function clean() { dirty = false; }
	function leave() { return !dirty || confirm("Kaydetmediğiniz değişiklik silinecek. Devam etmek istiyor musunuz?"); }
	function field(parent, label, name, value = "", type = "text", required = true) {
		const l = text("label", label); const input = document.createElement("input"); input.id = name; input.name = name; input.type = type; input.value = value; input.required = required;
		l.htmlFor = name; input.addEventListener("input", () => { if (session) dirty = true; }); parent.append(l, input); return input;
	}
	function select(parent, label, name, options, value) {
		const l = text("label", label); l.htmlFor = name; const e = document.createElement("select"); e.id = name;
		for (const [v, caption] of options) { const o = text("option", caption); o.value = v; e.append(o); }
		e.value = String(value); e.addEventListener("change", () => { dirty = true; }); parent.append(l, e); return e;
	}
	function details(parent, caption) { const d = document.createElement("details"); d.append(text("summary", caption)); parent.append(d); return d; }
	function form(parent, caption, submitText, action) {
		const f = document.createElement("form"); f.className = "kutu"; f.append(text("h2", caption)); parent.append(f);
		const actions = text("div", "", "form-dugmeleri"); const submit = text("button", submitText, "birincil"); submit.type = "submit"; actions.append(submit);
		f.addEventListener("submit", async event => {
			event.preventDefault(); if (busy) return; busy = true; submit.disabled = true; submit.textContent = submitText === "Giriş yap" ? "Giriş yapılıyor…" : "Kaydediliyor…";
			try { await action(); clean(); }
			catch (error) { status(error.message, true); }
			finally { busy = false; submit.disabled = false; submit.textContent = submitText; }
		});
		return { f, end: () => f.append(actions), actions };
	}
	async function api(path, body, method = body === undefined ? "GET" : "POST") {
		const headers = { "X-Hatim-Istek": "panel" }; if (session) headers["X-CSRF-Token"] = session.csrf;
		if (body !== undefined) headers["Content-Type"] = "application/json";
		let response;
		try { response = await fetch(`/api/${path}`, { method, credentials: "same-origin", cache: "no-store", headers, body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(15000) }); }
		catch { throw Error(method === "GET" ? "Bağlantı kurulamadı. Yeniden deneyin." : "Yanıt alınamadı. Değişiklik kaydedilmiş olabilir; listeyi yenileyip kontrol edin."); }
		let data; try { data = await response.json(); } catch { throw Error("Sunucudan yanıt alınamadı."); }
		if (!response.ok) { const error = Error(data.error || "Bilgiler geçersiz veya işlem tamamlanamadı."); error.status = response.status; throw error; }
		return data;
	}
	function saved(data, message = "Değişiklik kaydedildi.") { status(data.published === false ? `${message} Siteye aktarım gecikiyor; yönetici hizmet günlüklerini kontrol etmeli.` : message); }
	function navigation() { bul("oturum-nav").hidden = !session; bul("kullanicilar").hidden = session?.role !== "owner"; }
	function login(message = "Kullanıcı adınız ve parolanızla giriş yapın.") {
		session = null; clean(); navigation(); title("Giriş yap"); view.replaceChildren(); status(message);
		const block = form(view, "Yönetim paneline giriş", "Giriş yap", async () => {
			session = await api("login", { username: username.value, password: password.value, code: code.value }); password.value = ""; code.value = ""; navigation(); await list();
		});
		const username = field(block.f, "Kullanıcı adı", "username"); username.autocomplete = "username";
		const password = field(block.f, "Parola", "password", "", "password"); password.autocomplete = "current-password";
		const advanced = details(block.f, "Doğrulama kodu (açıksa)");
		const code = field(advanced, "Doğrulayıcı uygulamadaki 6 haneli kod", "login-code", "", "text", false); code.inputMode = "numeric"; code.autocomplete = "one-time-code"; code.maxLength = 6;
		block.end(); clean();
	}
	function schedule(parent, schedule, prefix) {
		const mode = select(parent, "Yenileme düzeni", `${prefix}-mode`, [["weekly", "Her hafta"], ["days", "Belirli gün aralığında"]], schedule.mode);
		const weekly = document.createElement("div"), days = document.createElement("div"); parent.append(weekly, days);
		const weekday = select(weekly, "Hangi gün?", `${prefix}-day`, [[0, "Pazartesi"], [1, "Salı"], [2, "Çarşamba"], [3, "Perşembe"], [4, "Cuma"], [5, "Cumartesi"], [6, "Pazar"]], schedule.weekday);
		const interval = field(days, "Kaç günde bir?", `${prefix}-interval`, schedule.interval_days, "number"); interval.min = 1; interval.max = 365;
		const time = field(parent, "Saat (Türkiye saati)", `${prefix}-time`, `${String(schedule.hour).padStart(2, "0")}:${String(schedule.minute).padStart(2, "0")}`, "time");
		const display = () => { weekly.hidden = mode.value !== "weekly"; days.hidden = mode.value !== "days"; mode.disabled = parent.hidden; time.disabled = parent.hidden; weekday.disabled = parent.hidden || weekly.hidden; interval.disabled = parent.hidden || days.hidden; }; mode.addEventListener("change", display); display();
		const read = () => { const [hour, minute] = time.value.split(":").map(Number); return { mode: mode.value, weekday: Number(weekday.value), hour, minute, interval_days: mode.value === "days" ? Number(interval.value) : 7 }; }; read.refresh = display; return read;
	}
	const defaultSchedule = () => ({ mode: "weekly", weekday: 1, hour: 21, minute: 0, interval_days: 7 });
	async function reloadMosques() { mosques = await api("mosques"); }
	async function list() {
		await reloadMosques(); clean(); activeMosque = activeGroup = null; view.replaceChildren(); title("Camilerim"); status(mosques.length ? "İşlem yapmak istediğiniz camiyi seçin." : "Henüz sorumlu olduğunuz cami yok.");
		const cards = text("div", "", "kartlar"); view.append(cards);
		for (const entry of mosques) { const b = button(entry.mosque.name, () => run(() => mosque(entry.mosque.id)), "kart"); b.append(text("small", `/${entry.mosque.slug}/`)); cards.append(b); }
		if (session.role === "owner") {
			const d = details(view, "Yeni cami ekle");
			const block = form(d, "Cami bilgileri", "Camiyi ekle", async () => { const data = await api("mosques", { name: name.value, slug: slug.value, schedule: defaultSchedule() }); await mosque(data.id); saved(data, "Cami eklendi. Şimdi hatim grubu ekleyebilirsiniz."); });
			const name = field(block.f, "Caminin adı", "new-mosque-name"); name.maxLength = 200;
			const slug = field(block.f, "Site adresindeki kısa adı", "new-mosque-slug"); slug.pattern = "[a-z0-9][a-z0-9-]{0,62}[a-z0-9]"; slug.maxLength = 64;
			block.f.append(text("p", "Örnek: htuc → site.com/htuc/", "kisa")); block.end();
		}
	}
	async function mosque(id) {
		await reloadMosques(); activeMosque = mosques.find(e => e.mosque.id === id); activeGroup = null; if (!activeMosque) throw Error("Bu camiye ulaşılamadı.");
		clean(); const m = activeMosque.mosque; view.replaceChildren(); title("Hatim grupları", m.name); status("İsimleri değiştirmek için hatim grubunu seçin.");
		const publicLink = text("a", "Caminin listesini aç", "dugme"); publicLink.href = `/${m.slug}/`; publicLink.target = "_blank"; publicLink.rel = "noopener"; view.append(publicLink);
		const cards = text("div", "", "kartlar"); view.append(cards);
		for (const g of activeMosque.groups) { const b = button(`${g.number}. grup`, () => run(() => group(g.id)), "kart"); b.append(text("small", g.name)); cards.append(b); }
		const add = details(view, "Yeni hatim grubu ekle");
		const create = form(add, "Grup bilgileri", "Grubu ekle", async () => { const data = await api(`mosques/${m.id}/groups`, { name: name.value, number: Number(number.value), schedule: null }); await group(data.id); saved(data, "Grup eklendi. Cüzleri seçip isimleri yazabilirsiniz."); });
		const number = field(create.f, "Grup numarası", "new-group-number", Math.max(0, ...activeMosque.groups.map(g => g.number)) + 1, "number"); number.min = 1; number.max = 99999;
		const name = field(create.f, "Grubun adı", "new-group-name", "Hatim grubu"); name.maxLength = 200; create.end();
		const settings = details(view, "Cami adı, adresi ve yenileme ayarları");
		const edit = form(settings, "Cami ayarları", "Ayarları kaydet", async () => { const data = await api(`mosques/${m.id}`, { name: mosqueName.value, slug: mosqueSlug.value, schedule: getSchedule() }, "PUT"); await mosque(m.id); saved(data); });
		const mosqueName = field(edit.f, "Caminin adı", "mosque-name", m.name); mosqueName.maxLength = 200;
		const mosqueSlug = field(edit.f, "Site adresindeki kısa adı", "mosque-slug", m.slug); mosqueSlug.maxLength = 64;
		edit.f.append(text("p", "Adres değiştiğinde eski adres kapanır. Kayıtlarınız korunur.", "kisa"));
		const getSchedule = schedule(edit.f, m.schedule, "mosque"); edit.f.append(text("p", "Döngü değişikliği sonraki yenileme tarihini yeniden belirler; cüzleri hemen ilerletmez.", "kisa")); edit.end();
		if (session.role === "owner") settings.append(button("Camiyi ve gruplarını sil", () => run(async () => { if (!confirm("Bu caminin bütün grupları ve okuyucu kayıtları silinecek. Emin misiniz?")) return; const data = await api(`mosques/${m.id}`, undefined, "DELETE"); await list(); saved(data, "Cami silindi."); }), "tehlike"));
	}
	async function group(id) {
		activeGroup = await api(`groups/${id}`); await reloadMosques(); activeMosque = mosques.find(e => e.mosque.id === activeGroup.group.mosque_id); if (!activeMosque) throw Error("Bu gruba ulaşılamadı.");
		clean(); const g = activeGroup.group, m = activeMosque.mosque; view.replaceChildren(); title(`${g.number}. grup · İsimleri düzenle`, m.name); status("Önce cüzü seçin, ardından ismi değiştirip kaydedin.");
		view.append(button("Hatim gruplarına dön", () => run(() => mosque(m.id))));
		const edit = form(view, "Cüzün okuyucusu", "İsmi kaydet", async () => {
			const reader = activeGroup.readers.find(r => r.id === pick.value); const data = await api(`groups/${g.id}/readers/${reader.id}`, { name: name.value, version: activeGroup.group.version }, "PUT");
			const selectedJuz = reader.juz; await group(g.id); const dropdown = bul("reader"); dropdown.value = activeGroup.readers.find(r => r.juz === selectedJuz).id; dropdown.dispatchEvent(new Event("change")); clean(); saved(data, `${selectedJuz}. cüz için isim kaydedildi.`);
		});
		const pick = select(edit.f, "1. Hangi cüz?", "reader", activeGroup.readers.map(r => [r.id, `${r.juz}. cüz`]), activeGroup.readers[0]?.id);
		const current = text("p", "", "kisa"); edit.f.append(current);
		const name = field(edit.f, "2. Okuyacak kişinin adı soyadı", "reader-name"); name.maxLength = 120; name.autocomplete = "off";
		let previous = pick.value;
		function selected() { const reader = activeGroup.readers.find(r => r.id === pick.value); name.value = reader?.name || ""; current.textContent = reader ? `Şu an kayıtlı: ${reader.name}` : "Henüz okuyucu yok."; name.disabled = !reader; previous = pick.value; clean(); }
		pick.addEventListener("change", () => { if (name.value !== activeGroup.readers.find(r => r.id === previous)?.name && !confirm("Kaydetmediğiniz isim silinecek. Başka cüz seçmek istiyor musunuz?")) { pick.value = previous; return; } selected(); }); selected();
		edit.actions.append(button("Değişikliği geri al", selected)); edit.end();
		view.append(button("Listeyi yeniden yükle", () => run(() => group(g.id))));
		const publicLink = text("a", "Hatim listesini aç", "dugme"); publicLink.href = `/${m.slug}/${g.number}/`; publicLink.target = "_blank"; publicLink.rel = "noopener"; view.append(publicLink);
		const options = details(view, "Grup adı ve yenileme ayarları");
		const settings = form(options, "Grup ayarları", "Ayarları kaydet", async () => { const data = await api(`groups/${g.id}`, { name: groupName.value, number: Number(groupNumber.value), schedule: inherit.value === "yes" ? null : getSchedule(), version: activeGroup.group.version }, "PUT"); await group(g.id); saved(data); });
		const groupName = field(settings.f, "Grubun adı", "group-name", g.name); groupName.maxLength = 200;
		const groupNumber = field(settings.f, "Grup numarası", "group-number", g.number, "number"); groupNumber.min = 1; groupNumber.max = 99999;
		const inherit = select(settings.f, "Yenileme düzeni nereden alınsın?", "inherit", [["yes", "Caminin ayarını kullan"], ["no", "Bu gruba özel ayar"]], g.schedule ? "no" : "yes");
		const own = document.createElement("div"); settings.f.append(own); const getSchedule = schedule(own, g.schedule || m.schedule, "group");
		const display = () => { own.hidden = inherit.value === "yes"; getSchedule.refresh(); }; inherit.addEventListener("change", display); display();
		const next = new Intl.DateTimeFormat("tr-TR", { dateStyle: "long", timeStyle: "short", timeZone: "Europe/Istanbul" }).format(new Date(g.next_due * 1000));
		settings.f.append(text("p", `Sonraki yenileme: ${next}`, "kisa")); settings.end();
		options.append(button("Bu grubu sil", () => run(async () => { if (!confirm("Bu grubun bütün okuyucu kayıtları silinecek. Emin misiniz?")) return; const data = await api(`groups/${g.id}`, undefined, "DELETE"); await mosque(m.id); saved(data, "Grup silindi."); }), "tehlike"));
	}
	async function account() {
		clean(); view.replaceChildren(); title("Hesabım"); status(`${session.username} hesabıyla giriş yaptınız.`);
		const change = form(view, "Parolayı değiştir", "Parolayı değiştir", async () => { if (next.value !== repeat.value) throw Error("Yeni parolalar aynı olmalı."); await api("password", { current_password: current.value, new_password: next.value, code: code.value }); login("Parolanız değişti. Yeni parolanızla giriş yapın."); });
		const current = field(change.f, "Mevcut parola", "current-password", "", "password"); current.autocomplete = "current-password";
		const next = field(change.f, "Yeni parola (en az 12 karakter)", "new-password", "", "password"); next.minLength = 12; next.maxLength = 128; next.autocomplete = "new-password";
		const repeat = field(change.f, "Yeni parola tekrar", "repeat-password", "", "password"); repeat.autocomplete = "new-password";
		const code = field(change.f, "Doğrulama kodu (iki aşamalı giriş açıksa)", "password-code", "", "text", false); code.inputMode = "numeric"; change.end();
		const d = details(view, `İki aşamalı giriş: ${session.mfa ? "açık" : "kapalı"}`);
		d.append(text("p", "Bir doğrulayıcı uygulama kullanıyorsanız girişinizi 6 haneli kodla da koruyabilirsiniz.", "kisa"));
		const setup = form(d, session.mfa ? "Doğrulayıcı uygulamayı değiştir" : "İki aşamalı girişi aç", "Kuruluma başla", async () => {
			const data = await api("mfa/begin", { password: pass.value, code: existing.value }); pass.value = "";
			const section = document.createElement("section"); d.append(section); section.append(text("p", "Bu anahtarı doğrulayıcı uygulamanıza ekleyin:"), text("code", data.secret, "anahtar"));
			const url = text("a", "Doğrulayıcı uygulamada aç", "dugme"); url.href = data.url; section.append(url);
			const finish = form(section, "Kurulumu doğrula", "İki aşamalı girişi aç", async () => { await api("mfa/finish", { code: verification.value }); login("İki aşamalı giriş açıldı. Yeni doğrulama koduyla tekrar giriş yapın."); });
			const verification = field(finish.f, "Uygulamadaki 6 haneli kod", "verification"); verification.inputMode = "numeric"; verification.autocomplete = "one-time-code"; finish.end(); status("Anahtarı ekleyip doğrulama kodunu yazın.");
		});
		const pass = field(setup.f, "Mevcut parola", "mfa-password", "", "password"); pass.autocomplete = "current-password";
		const existing = field(setup.f, "Mevcut doğrulama kodu (açıksa)", "existing-code", "", "text", false); existing.inputMode = "numeric"; setup.end();
		if (session.mfa) {
			const disable = form(d, "İki aşamalı girişi kapat", "İki aşamalı girişi kapat", async () => { await api("mfa/disable", { password: disablePass.value, code: disableCode.value }); login("İki aşamalı giriş kapatıldı. Parolanızla tekrar giriş yapın."); });
			const disablePass = field(disable.f, "Mevcut parola", "disable-password", "", "password"); const disableCode = field(disable.f, "Doğrulama kodu", "disable-code"); disable.end();
		}
	}
	async function users(selected = null) {
		await reloadMosques(); const all = await api("users"); clean(); view.replaceChildren(); title("Sunucu yönetimi"); status("Görevlilere sorumlu oldukları camileri atayın.");
		const cards = text("div", "", "kartlar"); view.append(cards);
		for (const user of all) cards.append(button(`${user.username}${user.disabled ? " (kapalı)" : ""}`, () => run(() => users(user.id)), "kart"));
		const user = all.find(u => u.id === selected);
		const block = form(view, user ? "Hesap ve yetkiler" : "Yeni görevli hesabı", user ? "Değişiklikleri kaydet" : "Hesabı oluştur", async () => {
			const payload = { username: username.value, password: password.value, role: role.value, mosque_ids: permissions.filter(([box]) => box.checked).map(([, id]) => id), disabled: disabled.checked };
			const data = await api(user ? `users/${user.id}` : "users", payload, user ? "PUT" : "POST");
			if (user?.username === session.username) { login("Hesap ayarları değişti. Yeniden giriş yapın."); return; }
			await users(); saved(data, user ? "Hesap güncellendi." : "Hesap oluşturuldu.");
		});
		const username = field(block.f, "Kullanıcı adı", "user-username", user?.username || ""); username.readOnly = !!user; username.autocomplete = "off";
		const password = field(block.f, user ? "Yeni parola (değiştirmeyecekseniz boş bırakın)" : "Parola (en az 12 karakter)", "user-password", "", "password", !user); password.minLength = 12; password.maxLength = 128; password.autocomplete = "new-password";
		const role = select(block.f, "Yetki", "user-role", [["manager", "Cami görevlisi · seçilen camiler"], ["owner", "Sunucu sahibi · tüm camiler"]], user?.role || "manager");
		const permissionArea = document.createElement("fieldset"); permissionArea.append(text("legend", "Sorumlu olduğu camiler")); block.f.append(permissionArea);
		const permissions = mosques.map(entry => { const label = text("label", ""); const box = document.createElement("input"); box.type = "checkbox"; box.checked = !!user?.mosque_ids.includes(entry.mosque.id); box.addEventListener("change", () => { dirty = true; }); label.append(box, document.createTextNode(entry.mosque.name)); permissionArea.append(label); return [box, entry.mosque.id]; });
		const display = () => { permissionArea.hidden = role.value === "owner"; }; role.addEventListener("change", display); display();
		const disabledLabel = text("label", ""); const disabled = document.createElement("input"); disabled.type = "checkbox"; disabled.checked = !!user?.disabled; disabledLabel.append(disabled, document.createTextNode("Bu hesabın girişini kapat")); block.f.append(disabledLabel); block.end();
		if (user) {
			const lastOwner = user.role === "owner" && !user.disabled && all.filter(u => u.role === "owner" && !u.disabled).length === 1;
			const remove = button("Kullanıcıyı sil", () => run(async () => {
				if (busy || !confirm(`“${user.username}” kullanıcısı kalıcı olarak silinecek. Oturumları ve cami yetkileri kaldırılacak. Cami ve hatim listeleri korunacak. Emin misiniz?`)) return;
				busy = true; remove.disabled = true;
				try {
					const data = await api(`users/${user.id}`, undefined, "DELETE"); clean();
					if (user.username === session.username) { login("Hesabınız silindi. Başka bir hesapla giriş yapabilirsiniz."); return; }
					await users(); saved(data, "Kullanıcı silindi.");
				} finally { busy = false; remove.disabled = lastOwner; }
			}), "tehlike");
			remove.disabled = lastOwner; block.actions.append(remove);
			if (lastOwner) block.f.append(text("p", "Son aktif sunucu sahibi hesabı silinemez.", "kisa"));
			view.append(button("Yeni görevli hesabı", () => run(() => users())));
		}
	}
	async function run(action) { if (!leave()) return; try { await action(); } catch (error) { if (error.status === 401) login("Oturumunuz sona erdi. Yeniden giriş yapın."); else status(error.message, true); } }
	bul("camilerim").addEventListener("click", () => run(list)); bul("hesabim").addEventListener("click", () => run(account)); bul("kullanicilar").addEventListener("click", () => run(() => users()));
	bul("cikis").addEventListener("click", () => run(async () => { await api("logout", {}); login("Çıkış yaptınız."); }));
	window.addEventListener("beforeunload", event => { if (dirty) { event.preventDefault(); event.returnValue = ""; } });
	(async () => { try { session = await api("session"); navigation(); await list(); } catch (error) { login(error.status === 401 ? undefined : error.message); } })();
})();
