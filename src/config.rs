use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::{
	net::SocketAddr,
	path::{Path, PathBuf},
};

#[derive(Clone, Deserialize, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
	pub listen: SocketAddr,
	pub origin: String,
	pub database_url: String,
	pub published_dir: PathBuf,
	pub timezone: String,
	pub secure_cookie: bool,
	pub session_hours: i64,
	pub idle_minutes: i64,
	pub scheduler_seconds: u64,
	pub trust_loopback_proxy: bool,
}
impl Default for Config {
	fn default() -> Self {
		Self {
			listen: "127.0.0.1:8787".parse().unwrap(),
			origin: "https://hatim.example.org".into(),
			database_url: "sqlite:///var/lib/hatimhane/hatim.sqlite3?mode=rwc".into(),
			published_dir: "/var/lib/hatimhane/public".into(),
			timezone: "Europe/Istanbul".into(),
			secure_cookie: true,
			session_hours: 8,
			idle_minutes: 30,
			scheduler_seconds: 15,
			trust_loopback_proxy: true,
		}
	}
}
impl Config {
	pub fn load(dir: &Path) -> Result<Self> {
		let mut files = std::fs::read_dir(dir)
			.with_context(|| format!("Yapılandırma dizini açılamadı: {}", dir.display()))?
			.filter_map(|e| e.ok().map(|e| e.path()))
			.filter(|p| p.extension().is_some_and(|v| v == "toml"))
			.collect::<Vec<_>>();
		files.sort();
		if files.is_empty() {
			bail!("Yapılandırma dizininde TOML dosyası bulunamadı");
		}
		let mut all = toml::Table::new();
		for file in files {
			let part: toml::Table = toml::from_str(&std::fs::read_to_string(&file)?)?;
			all.extend(part);
		}
		let config: Self = toml::Value::Table(all).try_into()?;
		config.validate()?;
		Ok(config)
	}
	pub fn validate(&self) -> Result<()> {
		let uri: axum::http::Uri = self.origin.parse()?;
		if uri.authority().is_none()
			|| uri.path_and_query().is_some_and(|p| p.as_str() != "/")
			|| self.origin.ends_with('/')
			|| self.origin.contains('@')
		{
			bail!("origin yalnızca şema ve alan adı içermeli; sonuna / koymayın");
		}
		if self.secure_cookie && uri.scheme_str() != Some("https") {
			bail!("Güvenli çerez için HTTPS origin zorunlu");
		}
		if !self.secure_cookie
			&& !(self.listen.ip().is_loopback()
				&& matches!(uri.host(), Some("localhost" | "127.0.0.1" | "[::1]")))
		{
			bail!("Güvensiz çerez yalnızca yerel denemede kullanılabilir");
		}
		self.timezone.parse::<chrono_tz::Tz>()?;
		if !(1..=24).contains(&self.session_hours)
			|| !(1..=120).contains(&self.idle_minutes)
			|| !(1..=60).contains(&self.scheduler_seconds)
		{
			bail!("Oturum veya zamanlayıcı sınırları geçersiz");
		}
		if !self.published_dir.is_absolute() {
			bail!("published_dir mutlak yol olmalı");
		}
		Ok(())
	}
	pub fn cookie_name(&self) -> &'static str {
		if self.secure_cookie {
			"__Host-hatim_session"
		} else {
			"hatim_session"
		}
	}
}
