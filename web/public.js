"use strict";
(() => {
	const bul = id => document.getElementById(id);
	const parts = location.pathname.split("/").filter(Boolean);
	let last = null;
	const link = (name, href, extra = "") => {
		const a = document.createElement("a"); a.className = "kart"; a.href = href; a.textContent = name;
		if (extra) { const s = document.createElement("small"); s.textContent = extra; a.append(s); }
		return a;
	};
	function status(text) { bul("durum").hidden = !text; bul("durum").textContent = text; }
	function show(data) {
		if (!parts.length) {
			bul("baslik").textContent = "Caminizi seçin";
			bul("aciklama").textContent = "Hatim listesini görmek için caminin adına dokunun.";
			bul("gruplar").replaceChildren(...data.map(m => link(m.name, `/${m.slug}/`)));
			status(data.length ? "" : "Henüz bir cami eklenmemiş."); return;
		}
		const mosque = data.mosque;
		bul("cami-adi").textContent = mosque.name; bul("cami-adi").hidden = false;
		document.title = `${mosque.name} | Hatimhane`;
		const number = parts[1] ? Number(parts[1]) : null;
		const group = data.groups.find(g => g.number === number) || (!number && data.groups.length === 1 ? data.groups[0] : null);
		bul("gruplar").replaceChildren(...data.groups.map(g => link(`${g.number}. grup`, `/${mosque.slug}/${g.number}/`, g.name)));
		bul("gruplar").hidden = data.groups.length <= 1 && !!group;
		bul("liste").hidden = !group;
		if (!group) { bul("baslik").textContent = "Hatim grubunuzu seçin"; status(data.groups.length ? "" : "Henüz bir hatim grubu eklenmemiş."); return; }
		bul("baslik").textContent = "Hatim listesi";
		bul("grup-adi").textContent = `${group.number}. grup · ${group.name}`;
		const format = new Intl.DateTimeFormat("tr-TR", { day: "numeric", month: "long", hour: "2-digit", minute: "2-digit", timeZone: data.timezone });
		bul("donem").textContent = `Sonraki yenileme: ${format.format(new Date(group.next_due * 1000))}`;
		const rows = group.readers.slice().sort((a, b) => a.juz - b.juz).map(reader => {
			const row = document.createElement("tr"); const name = document.createElement("td"); name.textContent = reader.name;
			const cell = document.createElement("td"); const juz = document.createElement("span"); juz.className = "cuz"; juz.textContent = `${reader.juz}. cüz`; cell.append(juz); row.append(name, cell); return row;
		});
		bul("katilimcilar").replaceChildren(...rows);
		status(Date.now() >= group.next_due * 1000 ? "Yeni döngünün listesi henüz güncellenmedi." : (rows.length ? "" : "Henüz okuyucu eklenmemiş."));
	}
	let busy = false;
	async function load() {
		if (busy) return; busy = true;
		try {
			const response = await fetch(`/veri/${parts[0] || "index"}.json`, { cache: "no-store", signal: AbortSignal.timeout(12000) });
			if (!response.ok) throw Error(); const data = await response.json(); last = data; show(data);
		} catch { status(last ? "Liste yenilenemedi. Son yüklenen liste gösteriliyor." : "Listeye ulaşılamadı. Sayfayı biraz sonra yenileyin."); }
		finally { busy = false; }
	}
	document.addEventListener("visibilitychange", () => { if (!document.hidden) load(); });
	setInterval(() => { if (!document.hidden) load(); }, 60000); load();
})();
