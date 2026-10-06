use crate::{
	App, AppError, AppResult,
	auth::{self, Session},
	model::*,
	publication,
};
use axum::{
	Json, Router,
	extract::{ConnectInfo, DefaultBodyLimit, Path, State},
	http::{HeaderMap, HeaderValue, StatusCode, header},
	middleware::{self, Next},
	response::{Html, IntoResponse, Response},
	routing::{get, post, put},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::{net::SocketAddr, sync::Arc};

type Shared = State<Arc<App>>;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Login {
	username: String,
	password: String,
	#[serde(default)]
	code: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MosqueInput {
	name: String,
	slug: String,
	schedule: Schedule,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GroupInput {
	name: String,
	number: i64,
	schedule: Option<Schedule>,
	#[serde(default)]
	version: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReaderInput {
	name: String,
	version: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UserInput {
	username: String,
	#[serde(default)]
	password: String,
	role: String,
	mosque_ids: Vec<String>,
	#[serde(default)]
	disabled: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PasswordInput {
	current_password: String,
	new_password: String,
	#[serde(default)]
	code: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MfaInput {
	#[serde(default)]
	password: String,
	#[serde(default)]
	code: String,
}

pub fn router(app: Arc<App>) -> Router {
	Router::new()
		.route("/", get(public_page))
		.route("/{slug}", get(public_page))
		.route("/{slug}/", get(public_page))
		.route("/{slug}/{number}", get(public_page))
		.route("/{slug}/{number}/", get(public_page))
		.route("/panel", get(panel))
		.route("/panel/", get(panel))
		.route("/assets/{file}", get(asset))
		.route("/veri/{file}", get(public_data))
		.route("/health", get(|| async { "ok" }))
		.route("/api/login", post(login))
		.route("/api/logout", post(logout))
		.route("/api/session", get(session_info))
		.route("/api/password", post(password))
		.route("/api/mfa/begin", post(mfa_begin))
		.route("/api/mfa/finish", post(mfa_finish))
		.route("/api/mfa/disable", post(mfa_disable))
		.route("/api/mosques", get(mosque_list).post(mosque_create))
		.route("/api/mosques/{id}", put(mosque_update).delete(mosque_delete))
		.route("/api/mosques/{id}/groups", post(group_create))
		.route(
			"/api/groups/{id}",
			get(group_get).put(group_update).delete(group_delete),
		)
		.route("/api/groups/{group}/readers/{reader}", put(reader_update))
		.route("/api/users", get(users).post(user_create))
		.route("/api/users/{id}", put(user_update).delete(user_delete))
		.layer(DefaultBodyLimit::max(16 * 1024))
		.layer(middleware::from_fn(headers))
		.with_state(app)
}
async fn headers(req: axum::extract::Request, next: Next) -> Response {
	let mut response = next.run(req).await;
	for (name, value) in [
		(
			"content-security-policy",
			"default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'; form-action 'self'",
		),
		("x-content-type-options", "nosniff"),
		("referrer-policy", "same-origin"),
		("permissions-policy", "camera=(), microphone=(), geolocation=()"),
		("cache-control", "no-store"),
		("x-frame-options", "DENY"),
	] {
		response.headers_mut().insert(
			axum::http::HeaderName::from_static(name),
			HeaderValue::from_static(value),
		);
	}
	response
}
async fn panel() -> Html<&'static str> {
	Html(include_str!("../web/panel.html"))
}
async fn asset(Path(file): Path<String>) -> AppResult<Response> {
	let (kind, body) = match file.as_str() {
		"favicon.svg" => ("image/svg+xml", include_str!("../web/favicon.svg")),
		"style.css" => ("text/css; charset=utf-8", include_str!("../web/style.css")),
		"theme.js" => (
			"application/javascript; charset=utf-8",
			include_str!("../web/theme.js"),
		),
		"public.js" => (
			"application/javascript; charset=utf-8",
			include_str!("../web/public.js"),
		),
		"panel.js" => (
			"application/javascript; charset=utf-8",
			include_str!("../web/panel.js"),
		),
		_ => return Err(AppError::missing()),
	};
	Ok(([(header::CONTENT_TYPE, kind)], body).into_response())
}
async fn public_page(State(app): Shared, uri: axum::http::Uri) -> AppResult<Html<&'static str>> {
	let parts = uri
		.path()
		.trim_matches('/')
		.split('/')
		.filter(|s| !s.is_empty())
		.collect::<Vec<_>>();
	if !parts.is_empty() {
		let p = app.publications.read().await;
		let data = p.get(parts[0]).ok_or_else(AppError::missing)?;
		if parts.len() > 1
			&& !parts[1]
				.parse::<i64>()
				.is_ok_and(|n| data.groups.iter().any(|g| g.number == n))
		{
			return Err(AppError::missing());
		}
	}
	Ok(Html(include_str!("../web/index.html")))
}
async fn public_data(State(app): Shared, Path(file): Path<String>) -> AppResult<Json<Value>> {
	let slug = file.strip_suffix(".json").ok_or_else(AppError::missing)?;
	let publications = app.publications.read().await;
	if slug == "index" {
		return Ok(Json(json!(
			publications
				.values()
				.map(|p| json!({"name":p.mosque.name,"slug":p.mosque.slug,"id":p.mosque.id}))
				.collect::<Vec<_>>()
		)));
	}
	Ok(Json(
		serde_json::to_value(publications.get(slug).ok_or_else(AppError::missing)?)
			.map_err(anyhow::Error::from)?,
	))
}
async fn limit(app: &App, key: String, max: u32) -> AppResult<()> {
	let mut limits = app.login_limits.lock().await;
	limits.retain(|_, (time, _)| *time + 900 > now());
	if limits.len() >= 10000 && !limits.contains_key(&key) {
		return Err(AppError::new(
			StatusCode::TOO_MANY_REQUESTS,
			"Çok fazla giriş denemesi. 15 dakika sonra deneyin.",
		));
	}
	let item = limits.entry(key).or_insert((now(), 0));
	item.1 += 1;
	if item.1 > max {
		return Err(AppError::new(
			StatusCode::TOO_MANY_REQUESTS,
			"Çok fazla giriş denemesi. 15 dakika sonra deneyin.",
		));
	}
	Ok(())
}
async fn login(
	State(app): Shared,
	ConnectInfo(peer): ConnectInfo<SocketAddr>,
	headers: HeaderMap,
	Json(input): Json<Login>,
) -> AppResult<Response> {
	auth::origin(&app, &headers)?;
	let username = input.username.trim().to_ascii_lowercase();
	if username.len() > 64 || input.password.len() > 512 || input.code.len() > 12 {
		return Err(AppError::bad("Giriş bilgisi çok uzun."));
	}
	let ip = if peer.ip().is_loopback() && app.config.trust_loopback_proxy {
		headers
			.get("x-real-ip")
			.and_then(|v| v.to_str().ok())
			.and_then(|v| v.parse::<std::net::IpAddr>().ok())
			.unwrap_or(peer.ip())
	} else {
		peer.ip()
	};
	limit(&app, format!("ip:{ip}"), 30).await?;
	limit(&app, format!("user:{username}"), 10).await?;
	let _guard = app.writes.lock().await;
	let row =
		sqlx::query(&app.db.sql(
			"SELECT id,password_hash,role,disabled,totp_secret,totp_last_step FROM users WHERE username=?",
		))
		.bind(&username)
		.fetch_optional(&app.db.pool)
		.await?;
	let hash = row
		.as_ref()
		.map(|r| r.get::<String, _>("password_hash"))
		.unwrap_or_else(|| app.dummy_hash.clone());
	let valid = auth::verify(input.password, hash).await;
	let failed = || {
		AppError::new(
			StatusCode::UNAUTHORIZED,
			"Kullanıcı adı, parola veya doğrulama kodu hatalı.",
		)
	};
	let row = row.ok_or_else(failed)?;
	if !valid || row.try_get::<i64, _>("disabled")? != 0 {
		return Err(failed());
	}
	let id: String = row.try_get("id")?;
	let secret: String = row.try_get("totp_secret")?;
	if !secret.is_empty() {
		let step = auth::totp_step(&secret, &username, &input.code, now()).ok_or_else(failed)?;
		if step <= row.try_get::<i64, _>("totp_last_step")? {
			return Err(failed());
		}
		sqlx::query(&app.db.sql("UPDATE users SET totp_last_step=? WHERE id=?"))
			.bind(step)
			.bind(&id)
			.execute(&app.db.pool)
			.await?;
	}
	let (token, csrf) = auth::new_session(&app, &id, &headers).await?;
	let body = json!({"username":username,"role":row.try_get::<String,_>("role")?,"csrf":csrf,"mfa":!secret.is_empty()});
	Ok((
		[(header::SET_COOKIE, auth::cookie(&app, &token, false))],
		Json(body),
	)
		.into_response())
}
async fn session_info(State(app): Shared, headers: HeaderMap) -> AppResult<Json<Value>> {
	let s = auth::session(&app, &headers, false).await?;
	Ok(Json(
		json!({"username":s.username,"role":s.role,"csrf":s.csrf,"mfa":s.mfa}),
	))
}
async fn logout(State(app): Shared, headers: HeaderMap) -> AppResult<Response> {
	let s = auth::session(&app, &headers, true).await?;
	sqlx::query(&app.db.sql("DELETE FROM sessions WHERE token_hash=?"))
		.bind(s.token_hash)
		.execute(&app.db.pool)
		.await?;
	Ok((
		[(header::SET_COOKIE, auth::cookie(&app, "", true))],
		Json(json!({"ok":true})),
	)
		.into_response())
}
async fn reauthenticate(app: &App, s: &Session, password: String, code: &str) -> AppResult<()> {
	if password.len() > 512 {
		return Err(AppError::bad("Parola çok uzun."));
	}
	limit(app, format!("reauth:{}", s.user_id), 10).await?;
	let row = sqlx::query(
		&app.db
			.sql("SELECT password_hash,totp_secret FROM users WHERE id=?"),
	)
	.bind(&s.user_id)
	.fetch_one(&app.db.pool)
	.await?;
	if !auth::verify(password, row.try_get("password_hash")?).await {
		return Err(AppError::new(
			StatusCode::UNAUTHORIZED,
			"Mevcut parola veya doğrulama kodu hatalı.",
		));
	}
	let secret: String = row.try_get("totp_secret")?;
	if !secret.is_empty() && auth::totp_step(&secret, &s.username, code, now()).is_none() {
		return Err(AppError::new(
			StatusCode::UNAUTHORIZED,
			"Mevcut parola veya doğrulama kodu hatalı.",
		));
	}
	Ok(())
}
async fn password(
	State(app): Shared,
	headers: HeaderMap,
	Json(input): Json<PasswordInput>,
) -> AppResult<Response> {
	let _guard = app.writes.lock().await;
	let s = auth::session(&app, &headers, true).await?;
	reauthenticate(&app, &s, input.current_password, &input.code).await?;
	if !auth::password_ok(&input.new_password) {
		return Err(AppError::bad("Yeni parola 12–128 karakter olmalı."));
	}
	let hash = tokio::task::spawn_blocking(move || auth::hash_password(&input.new_password))
		.await
		.map_err(anyhow::Error::from)??;
	let mut tx = app.db.pool.begin().await?;
	sqlx::query(&app.db.sql("UPDATE users SET password_hash=? WHERE id=?"))
		.bind(hash)
		.bind(&s.user_id)
		.execute(&mut *tx)
		.await?;
	sqlx::query(&app.db.sql("DELETE FROM sessions WHERE user_id=?"))
		.bind(&s.user_id)
		.execute(&mut *tx)
		.await?;
	tx.commit().await?;
	Ok((
		[(header::SET_COOKIE, auth::cookie(&app, "", true))],
		Json(json!({"ok":true})),
	)
		.into_response())
}
async fn mfa_begin(
	State(app): Shared,
	headers: HeaderMap,
	Json(input): Json<MfaInput>,
) -> AppResult<Json<Value>> {
	let _guard = app.writes.lock().await;
	let s = auth::session(&app, &headers, true).await?;
	reauthenticate(&app, &s, input.password, &input.code).await?;
	let secret = totp_rs::Secret::generate_secret().to_encoded().to_string();
	sqlx::query(&app.db.sql("UPDATE users SET totp_pending=? WHERE id=?"))
		.bind(&secret)
		.bind(&s.user_id)
		.execute(&app.db.pool)
		.await?;
	Ok(Json(
		json!({"secret":secret,"url":auth::totp(&secret,&s.username)?.get_url()}),
	))
}
async fn mfa_finish(
	State(app): Shared,
	headers: HeaderMap,
	Json(input): Json<MfaInput>,
) -> AppResult<Response> {
	let _guard = app.writes.lock().await;
	let s = auth::session(&app, &headers, true).await?;
	limit(&app, format!("mfa:{}", s.user_id), 10).await?;
	let secret: String = sqlx::query_scalar(&app.db.sql("SELECT totp_pending FROM users WHERE id=?"))
		.bind(&s.user_id)
		.fetch_one(&app.db.pool)
		.await?;
	let step = auth::totp_step(&secret, &s.username, &input.code, now())
		.ok_or_else(|| AppError::bad("Doğrulama kodu hatalı."))?;
	let mut tx = app.db.pool.begin().await?;
	sqlx::query(
		&app.db
			.sql("UPDATE users SET totp_secret=?,totp_pending='',totp_last_step=? WHERE id=?"),
	)
	.bind(secret)
	.bind(step)
	.bind(&s.user_id)
	.execute(&mut *tx)
	.await?;
	sqlx::query(&app.db.sql("DELETE FROM sessions WHERE user_id=?"))
		.bind(s.user_id)
		.execute(&mut *tx)
		.await?;
	tx.commit().await?;
	Ok((
		[(header::SET_COOKIE, auth::cookie(&app, "", true))],
		Json(json!({"ok":true})),
	)
		.into_response())
}
async fn mfa_disable(
	State(app): Shared,
	headers: HeaderMap,
	Json(input): Json<MfaInput>,
) -> AppResult<Response> {
	let _guard = app.writes.lock().await;
	let s = auth::session(&app, &headers, true).await?;
	reauthenticate(&app, &s, input.password, &input.code).await?;
	let mut tx = app.db.pool.begin().await?;
	sqlx::query(
		&app.db
			.sql("UPDATE users SET totp_secret='',totp_pending='',totp_last_step=-1 WHERE id=?"),
	)
	.bind(&s.user_id)
	.execute(&mut *tx)
	.await?;
	sqlx::query(&app.db.sql("DELETE FROM sessions WHERE user_id=?"))
		.bind(&s.user_id)
		.execute(&mut *tx)
		.await?;
	tx.commit().await?;
	Ok((
		[(header::SET_COOKIE, auth::cookie(&app, "", true))],
		Json(json!({"ok":true})),
	)
		.into_response())
}
async fn finish(app: &App, id: String) -> AppResult<Json<Value>> {
	let published = match publication::publish(app).await {
		Ok(()) => true,
		Err(e) => {
			tracing::error!("JSON yayını gecikti: {e}");
			false
		}
	};
	Ok(Json(json!({"ok":true,"id":id,"published":published})))
}
fn check_id(id: &str) -> AppResult<()> {
	if !uuid(id) {
		return Err(AppError::bad("Kayıt kimliği geçersiz."));
	}
	Ok(())
}
fn check_mosque(input: &MosqueInput) -> AppResult<()> {
	if !valid_name(&input.name, 200) || !valid_slug(&input.slug) {
		return Err(AppError::bad(
			"Cami adı ve adresi geçersiz. Adres 2–64 küçük harf, rakam veya kısa çizgi içerebilir.",
		));
	}
	input
		.schedule
		.validate()
		.map_err(|_| AppError::bad("Döngü bilgisi geçersiz."))?;
	Ok(())
}
fn check_group(input: &GroupInput) -> AppResult<()> {
	if !valid_name(&input.name, 200) || input.number < 1 || input.number > 99999 {
		return Err(AppError::bad("Grup adı veya numarası geçersiz."));
	}
	if let Some(s) = &input.schedule {
		s.validate()
			.map_err(|_| AppError::bad("Döngü bilgisi geçersiz."))?;
	}
	Ok(())
}
async fn mosque_list(State(app): Shared, headers: HeaderMap) -> AppResult<Json<Value>> {
	let s = auth::session(&app, &headers, false).await?;
	let mut list = Vec::new();
	for m in app.db.mosques().await? {
		if app.db.allowed(&s.user_id, &s.role, &m.id).await? {
			let groups = app.db.groups(&m.id).await?;
			list.push(json!({"mosque":m,"groups":groups}));
		}
	}
	Ok(Json(json!(list)))
}
async fn mosque_create(
	State(app): Shared,
	headers: HeaderMap,
	Json(input): Json<MosqueInput>,
) -> AppResult<Json<Value>> {
	let _guard = app.writes.lock().await;
	let s = auth::session(&app, &headers, true).await?;
	auth::owner(&s)?;
	check_mosque(&input)?;
	let id = uuid::Uuid::new_v4().to_string();
	sqlx::query(
		&app.db
			.sql("INSERT INTO mosques (id,name,slug,schedule) VALUES (?,?,?,?)"),
	)
	.bind(&id)
	.bind(input.name.trim())
	.bind(input.slug)
	.bind(serde_json::to_string(&input.schedule).map_err(anyhow::Error::from)?)
	.execute(&app.db.pool)
	.await?;
	finish(&app, id).await
}
async fn mosque_update(
	State(app): Shared,
	Path(id): Path<String>,
	headers: HeaderMap,
	Json(input): Json<MosqueInput>,
) -> AppResult<Json<Value>> {
	let _guard = app.writes.lock().await;
	let s = auth::session(&app, &headers, true).await?;
	check_id(&id)?;
	auth::authorize(&app, &s, &id).await?;
	check_mosque(&input)?;
	let old = app
		.db
		.mosques()
		.await?
		.into_iter()
		.find(|m| m.id == id)
		.ok_or_else(AppError::missing)?;
	let groups = app.db.groups(&id).await?;
	let mut tx = app.db.pool.begin().await?;
	sqlx::query(
		&app.db
			.sql("UPDATE mosques SET name=?,slug=?,schedule=? WHERE id=?"),
	)
	.bind(input.name.trim())
	.bind(input.slug)
	.bind(serde_json::to_string(&input.schedule).map_err(anyhow::Error::from)?)
	.bind(&id)
	.execute(&mut *tx)
	.await?;
	if old.schedule != input.schedule {
		for g in groups.into_iter().filter(|g| g.schedule.is_none()) {
			let due = input.schedule.next(
				now(),
				app.config.timezone.parse().map_err(anyhow::Error::from)?,
				true,
			)?;
			sqlx::query(
				&app.db
					.sql("UPDATE hatim_groups SET next_due=?,version=version+1 WHERE id=?"),
			)
			.bind(due)
			.bind(g.id)
			.execute(&mut *tx)
			.await?;
		}
	}
	tx.commit().await?;
	finish(&app, id).await
}
async fn find_group(app: &App, s: &Session, id: &str) -> AppResult<Group> {
	check_id(id)?;
	let mosque_id: String = sqlx::query_scalar(&app.db.sql("SELECT mosque_id FROM hatim_groups WHERE id=?"))
		.bind(id)
		.fetch_optional(&app.db.pool)
		.await?
		.ok_or_else(AppError::missing)?;
	auth::authorize(app, s, &mosque_id).await?;
	app.db
		.groups(&mosque_id)
		.await?
		.into_iter()
		.find(|g| g.id == id)
		.ok_or_else(AppError::missing)
}
async fn group_get(State(app): Shared, Path(id): Path<String>, headers: HeaderMap) -> AppResult<Json<Value>> {
	let s = auth::session(&app, &headers, false).await?;
	let group = find_group(&app, &s, &id).await?;
	Ok(Json(json!({"readers":app.db.readers(&id).await?,"group":group})))
}
async fn group_create(
	State(app): Shared,
	Path(id): Path<String>,
	headers: HeaderMap,
	Json(input): Json<GroupInput>,
) -> AppResult<Json<Value>> {
	let _guard = app.writes.lock().await;
	let s = auth::session(&app, &headers, true).await?;
	check_id(&id)?;
	auth::authorize(&app, &s, &id).await?;
	check_group(&input)?;
	let mosque = app
		.db
		.mosques()
		.await?
		.into_iter()
		.find(|m| m.id == id)
		.ok_or_else(AppError::missing)?;
	let due = input.schedule.as_ref().unwrap_or(&mosque.schedule).next(
		now(),
		app.config.timezone.parse().map_err(anyhow::Error::from)?,
		true,
	)?;
	let group_id = uuid::Uuid::new_v4().to_string();
	let mut tx = app.db.pool.begin().await?;
	sqlx::query(&app.db.sql("INSERT INTO hatim_groups (id,mosque_id,number,name,schedule,next_due,last_rotation,version) VALUES (?,?,?,?,?,?,?,1)")).bind(&group_id).bind(&id).bind(input.number).bind(input.name.trim()).bind(input.schedule.map(|v|serde_json::to_string(&v).unwrap()).unwrap_or_default()).bind(due).bind(now()).execute(&mut *tx).await?;
	for juz in 1..=30_i64 {
		sqlx::query(
			&app.db
				.sql("INSERT INTO readers (id,group_id,name,juz,position) VALUES (?,?,?,?,?)"),
		)
		.bind(uuid::Uuid::new_v4().to_string())
		.bind(&group_id)
		.bind("Okuyucu belirtilmedi")
		.bind(juz)
		.bind(juz)
		.execute(&mut *tx)
		.await?;
	}
	tx.commit().await?;
	finish(&app, group_id).await
}
async fn group_update(
	State(app): Shared,
	Path(id): Path<String>,
	headers: HeaderMap,
	Json(input): Json<GroupInput>,
) -> AppResult<Json<Value>> {
	let _guard = app.writes.lock().await;
	let s = auth::session(&app, &headers, true).await?;
	let old = find_group(&app, &s, &id).await?;
	check_group(&input)?;
	if old.version != input.version {
		return Err(AppError::stale());
	}
	let due = if old.schedule != input.schedule {
		let mosque = app
			.db
			.mosques()
			.await?
			.into_iter()
			.find(|m| m.id == old.mosque_id)
			.ok_or_else(AppError::missing)?;
		input.schedule.as_ref().unwrap_or(&mosque.schedule).next(
			now(),
			app.config.timezone.parse().map_err(anyhow::Error::from)?,
			true,
		)?
	} else {
		old.next_due
	};
	let result = sqlx::query(&app.db.sql(
		"UPDATE hatim_groups SET name=?,number=?,schedule=?,next_due=?,version=version+1 WHERE id=? AND version=?",
	))
	.bind(input.name.trim())
	.bind(input.number)
	.bind(
		input
			.schedule
			.map(|v| serde_json::to_string(&v).unwrap())
			.unwrap_or_default(),
	)
	.bind(due)
	.bind(&id)
	.bind(input.version)
	.execute(&app.db.pool)
	.await?;
	if result.rows_affected() != 1 {
		return Err(AppError::stale());
	}
	finish(&app, id).await
}
async fn reader_update(
	State(app): Shared,
	Path((group, reader)): Path<(String, String)>,
	headers: HeaderMap,
	Json(input): Json<ReaderInput>,
) -> AppResult<Json<Value>> {
	let _guard = app.writes.lock().await;
	let s = auth::session(&app, &headers, true).await?;
	let old = find_group(&app, &s, &group).await?;
	check_id(&reader)?;
	if !valid_name(&input.name, 120) {
		return Err(AppError::bad("Ad soyad 1–120 karakter olmalı."));
	}
	if old.version != input.version {
		return Err(AppError::stale());
	}
	let mut tx = app.db.pool.begin().await?;
	let result = sqlx::query(
		&app.db
			.sql("UPDATE hatim_groups SET version=version+1 WHERE id=? AND version=?"),
	)
	.bind(&group)
	.bind(input.version)
	.execute(&mut *tx)
	.await?;
	if result.rows_affected() != 1 {
		return Err(AppError::stale());
	}
	let result = sqlx::query(&app.db.sql("UPDATE readers SET name=? WHERE id=? AND group_id=?"))
		.bind(input.name.trim())
		.bind(reader)
		.bind(&group)
		.execute(&mut *tx)
		.await?;
	if result.rows_affected() != 1 {
		return Err(AppError::missing());
	}
	tx.commit().await?;
	finish(&app, group).await
}
async fn group_delete(
	State(app): Shared,
	Path(id): Path<String>,
	headers: HeaderMap,
) -> AppResult<Json<Value>> {
	let _guard = app.writes.lock().await;
	let s = auth::session(&app, &headers, true).await?;
	find_group(&app, &s, &id).await?;
	let mut tx = app.db.pool.begin().await?;
	sqlx::query(&app.db.sql("DELETE FROM readers WHERE group_id=?"))
		.bind(&id)
		.execute(&mut *tx)
		.await?;
	sqlx::query(&app.db.sql("DELETE FROM hatim_groups WHERE id=?"))
		.bind(&id)
		.execute(&mut *tx)
		.await?;
	tx.commit().await?;
	finish(&app, id).await
}
async fn mosque_delete(
	State(app): Shared,
	Path(id): Path<String>,
	headers: HeaderMap,
) -> AppResult<Json<Value>> {
	let _guard = app.writes.lock().await;
	let s = auth::session(&app, &headers, true).await?;
	auth::owner(&s)?;
	check_id(&id)?;
	let groups = app.db.groups(&id).await?;
	let mut tx = app.db.pool.begin().await?;
	for g in groups {
		sqlx::query(&app.db.sql("DELETE FROM readers WHERE group_id=?"))
			.bind(g.id)
			.execute(&mut *tx)
			.await?;
	}
	sqlx::query(&app.db.sql("DELETE FROM hatim_groups WHERE mosque_id=?"))
		.bind(&id)
		.execute(&mut *tx)
		.await?;
	sqlx::query(&app.db.sql("DELETE FROM permissions WHERE mosque_id=?"))
		.bind(&id)
		.execute(&mut *tx)
		.await?;
	let result = sqlx::query(&app.db.sql("DELETE FROM mosques WHERE id=?"))
		.bind(&id)
		.execute(&mut *tx)
		.await?;
	if result.rows_affected() != 1 {
		return Err(AppError::missing());
	}
	tx.commit().await?;
	finish(&app, id).await
}
async fn users(State(app): Shared, headers: HeaderMap) -> AppResult<Json<Value>> {
	let s = auth::session(&app, &headers, false).await?;
	auth::owner(&s)?;
	let mut users = Vec::new();
	for row in sqlx::query("SELECT id,username,role,disabled,totp_secret FROM users ORDER BY username")
		.fetch_all(&app.db.pool)
		.await?
	{
		let id: String = row.try_get("id")?;
		let mosque_ids: Vec<String> = sqlx::query_scalar(
			&app.db
				.sql("SELECT mosque_id FROM permissions WHERE user_id=? ORDER BY mosque_id"),
		)
		.bind(&id)
		.fetch_all(&app.db.pool)
		.await?;
		users.push(json!({"id":id,"username":row.try_get::<String,_>("username")?,"role":row.try_get::<String,_>("role")?,"disabled":row.try_get::<i64,_>("disabled")?!=0,"mfa":!row.try_get::<String,_>("totp_secret")?.is_empty(),"mosque_ids":mosque_ids}));
	}
	Ok(Json(json!(users)))
}
fn valid_username(v: &str) -> bool {
	(3..=64).contains(&v.len())
		&& v.bytes()
			.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || [b'-', b'_', b'.'].contains(&b))
}
async fn user_checks(app: &App, input: &UserInput, create: bool) -> AppResult<()> {
	if !valid_username(&input.username) || !matches!(input.role.as_str(), "owner" | "manager") {
		return Err(AppError::bad("Kullanıcı adı veya yetki geçersiz."));
	}
	if (create || !input.password.is_empty()) && !auth::password_ok(&input.password) {
		return Err(AppError::bad("Parola 12–128 karakter olmalı."));
	}
	let mosques = app.db.mosques().await?;
	if input
		.mosque_ids
		.iter()
		.any(|id| !uuid(id) || !mosques.iter().any(|m| &m.id == id))
		|| input
			.mosque_ids
			.iter()
			.collect::<std::collections::HashSet<_>>()
			.len() != input.mosque_ids.len()
	{
		return Err(AppError::bad("Sorumlu cami listesi geçersiz."));
	}
	Ok(())
}
async fn user_create(
	State(app): Shared,
	headers: HeaderMap,
	Json(input): Json<UserInput>,
) -> AppResult<Json<Value>> {
	let _guard = app.writes.lock().await;
	let s = auth::session(&app, &headers, true).await?;
	auth::owner(&s)?;
	user_checks(&app, &input, true).await?;
	let password = input.password;
	let hash = tokio::task::spawn_blocking(move || auth::hash_password(&password))
		.await
		.map_err(anyhow::Error::from)??;
	let id = uuid::Uuid::new_v4().to_string();
	let mut tx = app.db.pool.begin().await?;
	sqlx::query(
		&app.db
			.sql("INSERT INTO users (id,username,password_hash,role,disabled) VALUES (?,?,?,?,?)"),
	)
	.bind(&id)
	.bind(input.username)
	.bind(hash)
	.bind(input.role)
	.bind(i64::from(input.disabled))
	.execute(&mut *tx)
	.await?;
	for mosque in input.mosque_ids {
		sqlx::query(
			&app.db
				.sql("INSERT INTO permissions (user_id,mosque_id) VALUES (?,?)"),
		)
		.bind(&id)
		.bind(mosque)
		.execute(&mut *tx)
		.await?;
	}
	tx.commit().await?;
	Ok(Json(json!({"ok":true,"id":id})))
}
async fn user_update(
	State(app): Shared,
	Path(id): Path<String>,
	headers: HeaderMap,
	Json(input): Json<UserInput>,
) -> AppResult<Json<Value>> {
	let _guard = app.writes.lock().await;
	let s = auth::session(&app, &headers, true).await?;
	auth::owner(&s)?;
	check_id(&id)?;
	user_checks(&app, &input, false).await?;
	let old = sqlx::query(&app.db.sql("SELECT username,role,disabled FROM users WHERE id=?"))
		.bind(&id)
		.fetch_optional(&app.db.pool)
		.await?
		.ok_or_else(AppError::missing)?;
	if input.username != old.try_get::<String, _>("username")? {
		return Err(AppError::bad("Kullanıcı adı bu işlemde değiştirilemez."));
	}
	if old.try_get::<String, _>("role")? == "owner"
		&& old.try_get::<i64, _>("disabled")? == 0
		&& (input.role != "owner" || input.disabled)
	{
		let owners: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role='owner' AND disabled=0")
			.fetch_one(&app.db.pool)
			.await?;
		if owners <= 1 {
			return Err(AppError::bad(
				"Son sunucu sahibi hesabı kapatılamaz veya yetkisi kaldırılamaz.",
			));
		}
	}
	let hash = if input.password.is_empty() {
		None
	} else {
		Some(
			tokio::task::spawn_blocking(move || auth::hash_password(&input.password))
				.await
				.map_err(anyhow::Error::from)??,
		)
	};
	let mut tx = app.db.pool.begin().await?;
	sqlx::query(&app.db.sql("UPDATE users SET role=?,disabled=? WHERE id=?"))
		.bind(input.role)
		.bind(i64::from(input.disabled))
		.bind(&id)
		.execute(&mut *tx)
		.await?;
	if let Some(hash) = hash {
		sqlx::query(&app.db.sql("UPDATE users SET password_hash=? WHERE id=?"))
			.bind(hash)
			.bind(&id)
			.execute(&mut *tx)
			.await?;
	}
	sqlx::query(&app.db.sql("DELETE FROM permissions WHERE user_id=?"))
		.bind(&id)
		.execute(&mut *tx)
		.await?;
	for mosque in input.mosque_ids {
		sqlx::query(
			&app.db
				.sql("INSERT INTO permissions (user_id,mosque_id) VALUES (?,?)"),
		)
		.bind(&id)
		.bind(mosque)
		.execute(&mut *tx)
		.await?;
	}
	sqlx::query(&app.db.sql("DELETE FROM sessions WHERE user_id=?"))
		.bind(&id)
		.execute(&mut *tx)
		.await?;
	tx.commit().await?;
	Ok(Json(json!({"ok":true,"id":id})))
}

async fn user_delete(State(app): Shared, Path(id): Path<String>, headers: HeaderMap) -> AppResult<Response> {
	let _guard = app.writes.lock().await;
	let s = auth::session(&app, &headers, true).await?;
	auth::owner(&s)?;
	check_id(&id)?;
	let old = sqlx::query(&app.db.sql("SELECT role,disabled FROM users WHERE id=?"))
		.bind(&id)
		.fetch_optional(&app.db.pool)
		.await?
		.ok_or_else(AppError::missing)?;
	if old.try_get::<String, _>("role")? == "owner" && old.try_get::<i64, _>("disabled")? == 0 {
		let owners: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role='owner' AND disabled=0")
			.fetch_one(&app.db.pool)
			.await?;
		if owners <= 1 {
			return Err(AppError::bad("Son aktif sunucu sahibi hesabı silinemez."));
		}
	}
	let mut tx = app.db.pool.begin().await?;
	for statement in [
		"DELETE FROM sessions WHERE user_id=?",
		"DELETE FROM permissions WHERE user_id=?",
		"DELETE FROM users WHERE id=?",
	] {
		sqlx::query(&app.db.sql(statement))
			.bind(&id)
			.execute(&mut *tx)
			.await?;
	}
	tx.commit().await?;
	let mut response = Json(json!({"ok":true,"id":id})).into_response();
	if id == s.user_id {
		response.headers_mut().insert(
			header::SET_COOKIE,
			HeaderValue::from_str(&auth::cookie(&app, "", true)).map_err(anyhow::Error::from)?,
		);
	}
	Ok(response)
}
