"use strict";
(() => {
	const anahtar = "hatim-tema";
	const cihaz = window.matchMedia("(prefers-color-scheme: dark)");
	let tercih = null;
	try {
		const kayit = localStorage.getItem(anahtar);
		if (kayit === "light" || kayit === "dark") tercih = kayit;
	} catch { /* Depolama kapalı olsa da tema çalışır. */ }
	function uygula() {
		const tema = tercih || (cihaz.matches ? "dark" : "light");
		document.documentElement.dataset.tema = tema;
		const etiket = document.getElementById("tema-etiketi");
		if (etiket) etiket.textContent = tema === "dark" ? "Açık görünüm" : "Koyu görünüm";
		const otomatik = document.getElementById("otomatik-tema");
		if (otomatik) otomatik.hidden = !tercih;
	}
	function kaydet() {
		try {
			if (tercih) localStorage.setItem(anahtar, tercih);
			else localStorage.removeItem(anahtar);
		} catch { /* Tercih bu sekmede kullanılmaya devam eder. */ }
		uygula();
	}
	uygula();
	cihaz.addEventListener("change", uygula);
	window.addEventListener("storage", (olay) => {
		if (olay.key !== anahtar && olay.key !== null) return;
		tercih = olay.newValue === "light" || olay.newValue === "dark" ? olay.newValue : null;
		uygula();
	});
	document.addEventListener("DOMContentLoaded", () => {
		uygula();
		document.getElementById("tema-dugmesi").addEventListener("click", () => {
			tercih = document.documentElement.dataset.tema === "dark" ? "light" : "dark";
			kaydet();
		});
		document.getElementById("otomatik-tema").addEventListener("click", () => {
			tercih = null;
			kaydet();
		});
	});
})();
