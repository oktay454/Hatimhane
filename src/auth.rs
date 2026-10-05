use crate::{App, AppError, AppResult, model::now};
use argon2::{
	Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version,
	password_hash::SaltString,
};
use axum::http::{HeaderMap, StatusCode};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{RngCore, rngs::OsRng};
use sha2::{Digest, Sha256};
use sqlx::Row;
use totp_rs::{Algorithm as TotpAlgorithm, Secret, TOTP};

#[derive(Clone)]
pub struct Session {
	pub user_id: String,
	pub username: String,
	pub role: String,
	pub csrf: String,
	pub token_hash: String,
	pub mfa: bool,
}
pub fn token() -> String {
	let mut bytes = [0; 32];
	OsRng.fill_bytes(&mut bytes);
	URL_SAFE_NO_PAD.encode(bytes)
}
pub fn digest(value: &str) -> String {
	format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn argon() -> Argon2<'static> {
	Argon2::new(
		Algorithm::Argon2id,
		Version::V0x13,
		Params::new(19456, 2, 1, None).unwrap(),
	)
}
pub fn password_ok(value: &str) -> bool {
	(12..=128).contains(&value.chars().count()) && value.len() <= 512
}
pub fn hash_password(value: &str) -> anyhow::Result<String> {
	if !password_ok(value) {
		anyhow::bail!("Parola 12–128 karakter olmalı");
	}
	Ok(argon()
		.hash_password(value.as_bytes(), &SaltString::generate(&mut OsRng))
		.map_err(|_| anyhow::anyhow!("Parola özeti üretilemedi"))?
		.to_string())
}
pub async fn verify(value: String, hash: String) -> bool {
	tokio::task::spawn_blocking(move || {
		PasswordHash::new(&hash).is_ok_and(|h| argon().verify_password(value.as_bytes(), &h).is_ok())
	})
	.await
	.unwrap_or(false)
}
pub fn totp(secret: &str, username: &str) -> anyhow::Result<TOTP> {
	Ok(TOTP::new(
		TotpAlgorithm::SHA1,
		6,
		1,
		30,
		Secret::Encoded(secret.to_owned())
			.to_bytes()
			.map_err(|_| anyhow::anyhow!("TOTP anahtarı geçersiz"))?,
		Some("Hatimhane".into()),
		username.into(),
	)?)
}
pub fn totp_step(secret: &str, username: &str, code: &str, at: i64) -> Option<i64> {
	if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) || at < 30 {
		return None;
	}
	let t = totp(secret, username).ok()?;
	let step = at / 30;
	for s in [step, step - 1, step + 1] {
		if constant_time_eq::constant_time_eq(t.generate((s * 30) as u64).as_bytes(), code.as_bytes()) {
			return Some(s);
		}
	}
	None
}
pub fn origin(app: &App, headers: &HeaderMap) -> AppResult<()> {
	if headers.get("origin").and_then(|v| v.to_str().ok()) != Some(app.config.origin.as_str())
		|| headers.get("x-hatim-istek").and_then(|v| v.to_str().ok()) != Some("panel")
	{
		return Err(AppError::new(
			StatusCode::FORBIDDEN,
			"İstek doğrulanamadı. Sayfayı yenileyin.",
		));
	}
	Ok(())
}
pub async fn session(app: &App, headers: &HeaderMap, write: bool) -> AppResult<Session> {
	let value = headers
		.get("cookie")
		.and_then(|v| v.to_str().ok())
		.unwrap_or("")
		.split(';')
		.filter_map(|v| v.trim().split_once('='))
		.find(|(key, _)| *key == app.config.cookie_name())
		.map(|(_, v)| v)
		.ok_or_else(AppError::unauthorized)?;
	if value.len() != 43 {
		return Err(AppError::unauthorized());
	}
	let token_hash = digest(value);
	let row=sqlx::query(&app.db.sql("SELECT u.id,u.username,u.role,u.disabled,u.totp_secret,s.csrf,s.expires_at,s.last_seen FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash=?")).bind(&token_hash).fetch_optional(&app.db.pool).await?.ok_or_else(AppError::unauthorized)?;
	if row.try_get::<i64, _>("disabled")? != 0
		|| row.try_get::<i64, _>("expires_at")? <= now()
		|| row.try_get::<i64, _>("last_seen")? + app.config.idle_minutes * 60 <= now()
	{
		return Err(AppError::unauthorized());
	}
	let s = Session {
		user_id: row.try_get("id")?,
		username: row.try_get("username")?,
		role: row.try_get("role")?,
		csrf: row.try_get("csrf")?,
		token_hash: token_hash.clone(),
		mfa: !row.try_get::<String, _>("totp_secret")?.is_empty(),
	};
	if write {
		origin(app, headers)?;
		if headers.get("x-csrf-token").and_then(|v| v.to_str().ok()) != Some(s.csrf.as_str()) {
			return Err(AppError::new(
				StatusCode::FORBIDDEN,
				"Oturum doğrulanamadı. Sayfayı yenileyin.",
			));
		}
	}
	sqlx::query(&app.db.sql("UPDATE sessions SET last_seen=? WHERE token_hash=?"))
		.bind(now())
		.bind(token_hash)
		.execute(&app.db.pool)
		.await?;
	Ok(s)
}
pub async fn authorize(app: &App, session: &Session, mosque_id: &str) -> AppResult<()> {
	if !app.db.allowed(&session.user_id, &session.role, mosque_id).await? {
		return Err(AppError::forbidden());
	}
	Ok(())
}
pub fn owner(session: &Session) -> AppResult<()> {
	if session.role != "owner" {
		return Err(AppError::forbidden());
	}
	Ok(())
}
pub async fn new_session(app: &App, user_id: &str, headers: &HeaderMap) -> AppResult<(String, String)> {
	// Aynı tarayıcının önceki oturumu yeni girişte iptal edilir.
	if let Ok(old) = session(app, headers, false).await {
		sqlx::query(&app.db.sql("DELETE FROM sessions WHERE token_hash=?"))
			.bind(old.token_hash)
			.execute(&app.db.pool)
			.await?;
	}
	let (id, csrf) = (token(), token());
	sqlx::query(
		&app.db
			.sql("INSERT INTO sessions (token_hash,user_id,csrf,expires_at,last_seen) VALUES (?,?,?,?,?)"),
	)
	.bind(digest(&id))
	.bind(user_id)
	.bind(&csrf)
	.bind(now() + app.config.session_hours * 3600)
	.bind(now())
	.execute(&app.db.pool)
	.await?;
	Ok((id, csrf))
}
pub fn cookie(app: &App, value: &str, clear: bool) -> String {
	format!(
		"{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
		app.config.cookie_name(),
		value,
		if clear { 0 } else { app.config.session_hours * 3600 },
		if app.config.secure_cookie { "; Secure" } else { "" }
	)
}
#[cfg(test)]
mod tests {
	use super::*;
	#[tokio::test]
	async fn hash_and_verify() {
		let hash = hash_password("uzun-test-parolasi").unwrap();
		assert!(hash.starts_with("$argon2id$"));
		assert!(verify("uzun-test-parolasi".into(), hash.clone()).await);
		assert!(!verify("yanlis-test-parolasi".into(), hash).await);
	}
	#[test]
	fn token_entropy_and_totp() {
		let a = token();
		assert_eq!(a.len(), 43);
		assert_ne!(a, token());
		let secret = Secret::generate_secret().to_encoded().to_string();
		let at = 1_790_000_000;
		let code = totp(&secret, "deneme").unwrap().generate(at as u64);
		assert_eq!(totp_step(&secret, "deneme", &code, at), Some(at / 30));
		assert_eq!(totp_step(&secret, "deneme", "abcdef", at), None);
	}
}
