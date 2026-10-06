use axum::{
	body::Body,
	extract::ConnectInfo,
	http::{Request, StatusCode},
};
use hatimhane::{App, auth, config::Config, model::*, publication, routes};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
use tower::ServiceExt;

struct Fixture {
	app: Arc<App>,
	root: PathBuf,
	username: String,
}
impl Fixture {
	async fn new() -> Self {
		Self::with_url(None).await
	}
	async fn with_url(url: Option<String>) -> Self {
		let root = std::env::current_dir()
			.unwrap()
			.join("work/tests")
			.join(uuid::Uuid::new_v4().to_string());
		std::fs::create_dir_all(&root).unwrap();
		let config = Config {
			listen: "127.0.0.1:18767".parse().unwrap(),
			origin: "http://127.0.0.1:18767".into(),
			secure_cookie: false,
			database_url: url
				.unwrap_or_else(|| format!("sqlite://{}?mode=rwc", root.join("test.sqlite3").display())),
			published_dir: root.join("public"),
			..Default::default()
		};
		let app = App::new(config).await.unwrap();
		let username = format!("owner-{}", uuid::Uuid::new_v4());
		let hash = auth::hash_password("Yalnizca-Test-123").unwrap();
		sqlx::query(
			&app.db
				.sql("INSERT INTO users (id,username,password_hash,role) VALUES (?,?,?,'owner')"),
		)
		.bind(uuid::Uuid::new_v4().to_string())
		.bind(&username)
		.bind(hash)
		.execute(&app.db.pool)
		.await
		.unwrap();
		Self { app, root, username }
	}
	async fn request(
		&self,
		method: &str,
		path: &str,
		body: Option<Value>,
		cookie: Option<&str>,
		csrf: Option<&str>,
		origin: bool,
	) -> (StatusCode, Value, Option<String>) {
		let mut builder = Request::builder().method(method).uri(path);
		if origin {
			builder = builder
				.header("origin", &self.app.config.origin)
				.header("x-hatim-istek", "panel");
		}
		if let Some(cookie) = cookie {
			builder = builder.header("cookie", cookie);
		}
		if let Some(csrf) = csrf {
			builder = builder.header("x-csrf-token", csrf);
		}
		if body.is_some() {
			builder = builder.header("content-type", "application/json");
		}
		let mut req = builder
			.body(Body::from(body.map(|v| v.to_string()).unwrap_or_default()))
			.unwrap();
		req.extensions_mut().insert(ConnectInfo(
			"127.0.0.1:32100".parse::<std::net::SocketAddr>().unwrap(),
		));
		let response = routes::router(self.app.clone()).oneshot(req).await.unwrap();
		let status = response.status();
		let cookie = response
			.headers()
			.get("set-cookie")
			.map(|v| v.to_str().unwrap().split(';').next().unwrap().to_owned());
		let bytes = response.into_body().collect().await.unwrap().to_bytes();
		let data = serde_json::from_slice(&bytes)
			.unwrap_or_else(|_| json!(String::from_utf8_lossy(&bytes).to_string()));
		(status, data, cookie)
	}
	async fn login(&self) -> (String, String) {
		let (status, data, cookie) = self
			.request(
				"POST",
				"/api/login",
				Some(json!({"username":self.username,"password":"Yalnizca-Test-123","code":""})),
				None,
				None,
				true,
			)
			.await;
		assert_eq!(status, StatusCode::OK);
		(cookie.unwrap(), data["csrf"].as_str().unwrap().into())
	}
	async fn mosque(&self, cookie: &str, csrf: &str, slug: &str) -> String {
		let (status, data, _) = self
			.request(
				"POST",
				"/api/mosques",
				Some(json!({"name":format!("Cami {slug}"),"slug":slug,"schedule":Schedule::default()})),
				Some(cookie),
				Some(csrf),
				true,
			)
			.await;
		assert_eq!(status, StatusCode::OK, "{data}");
		data["id"].as_str().unwrap().into()
	}
	async fn group(&self, cookie: &str, csrf: &str, mosque: &str, number: i64) -> String {
		let (status, data, _) = self
			.request(
				"POST",
				&format!("/api/mosques/{mosque}/groups"),
				Some(json!({"name":"Deneme grubu","number":number,"schedule":null})),
				Some(cookie),
				Some(csrf),
				true,
			)
			.await;
		assert_eq!(status, StatusCode::OK, "{data}");
		data["id"].as_str().unwrap().into()
	}
}
impl Drop for Fixture {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.root);
	}
}

#[tokio::test]
async fn scope_is_enforced_on_every_write() {
	let f = Fixture::new().await;
	let (cookie, csrf) = f.login().await;
	let a = f.mosque(&cookie, &csrf, "birinci").await;
	let b = f.mosque(&cookie, &csrf, "ikinci").await;
	let ga = f.group(&cookie, &csrf, &a, 1).await;
	let gb = f.group(&cookie, &csrf, &b, 1).await;
	let (status,_,_)=f.request("POST","/api/users",Some(json!({"username":"gorevli","password":"Yalnizca-Test-123","role":"manager","mosque_ids":[a],"disabled":false})),Some(&cookie),Some(&csrf),true).await;
	assert_eq!(status, StatusCode::OK);
	let (status, data, manager) = f
		.request(
			"POST",
			"/api/login",
			Some(json!({"username":"gorevli","password":"Yalnizca-Test-123"})),
			None,
			None,
			true,
		)
		.await;
	assert_eq!(status, StatusCode::OK);
	let manager = manager.unwrap();
	let token = data["csrf"].as_str().unwrap();
	let (status, list, _) = f
		.request("GET", "/api/mosques", None, Some(&manager), None, false)
		.await;
	assert_eq!(status, StatusCode::OK);
	assert_eq!(list.as_array().unwrap().len(), 1);
	assert_eq!(
		f.request(
			"GET",
			&format!("/api/groups/{gb}"),
			None,
			Some(&manager),
			None,
			false
		)
		.await
		.0,
		StatusCode::FORBIDDEN
	);
	assert_eq!(
		f.request(
			"PUT",
			&format!("/api/mosques/{b}"),
			Some(json!({"name":"Yetkisiz","slug":"yetkisiz","schedule":Schedule::default()})),
			Some(&manager),
			Some(token),
			true
		)
		.await
		.0,
		StatusCode::FORBIDDEN
	);
	assert_eq!(
		f.request("GET", "/api/users", None, Some(&manager), None, false)
			.await
			.0,
		StatusCode::FORBIDDEN
	);
	assert_eq!(
		f.request(
			"POST",
			"/api/mosques",
			Some(json!({"name":"Yetkisiz","slug":"yetkisiz","schedule":Schedule::default()})),
			Some(&manager),
			Some(token),
			true
		)
		.await
		.0,
		StatusCode::FORBIDDEN
	);
	let (_, group, _) = f
		.request(
			"GET",
			&format!("/api/groups/{ga}"),
			None,
			Some(&manager),
			None,
			false,
		)
		.await;
	let reader = &group["readers"][0];
	assert_eq!(
		f.request(
			"PUT",
			&format!("/api/groups/{ga}/readers/{}", reader["id"].as_str().unwrap()),
			Some(json!({"name":"Görevli Okuyucusu","version":1})),
			Some(&manager),
			Some(token),
			true
		)
		.await
		.0,
		StatusCode::OK
	);
	let (_, data, _) = f
		.request("GET", "/veri/birinci.json", None, None, None, false)
		.await;
	assert_eq!(data["groups"][0]["readers"][0]["name"], "Görevli Okuyucusu");
}
#[tokio::test]
async fn uri_collisions_and_uuid_survive_rename() {
	let f = Fixture::new().await;
	let (cookie, csrf) = f.login().await;
	let id = f.mosque(&cookie, &csrf, "htuc").await;
	let _other = f.mosque(&cookie, &csrf, "baska").await;
	let group = f.group(&cookie, &csrf, &id, 1).await;
	f.group(&cookie, &csrf, &id, 2).await;
	let input =
		|slug: &str| json!({"name":"Hasan Tahsin Uğur Camii","slug":slug,"schedule":Schedule::default()});
	assert_eq!(
		f.request(
			"PUT",
			&format!("/api/mosques/{id}"),
			Some(input("baska")),
			Some(&cookie),
			Some(&csrf),
			true
		)
		.await
		.0,
		StatusCode::CONFLICT
	);
	assert_eq!(
		f.request(
			"PUT",
			&format!("/api/mosques/{id}"),
			Some(input("api")),
			Some(&cookie),
			Some(&csrf),
			true
		)
		.await
		.0,
		StatusCode::BAD_REQUEST
	);
	let before = f.app.db.readers(&group).await.unwrap();
	assert_eq!(
		f.request(
			"PUT",
			&format!("/api/mosques/{id}"),
			Some(input("hasantahsinugurcamii")),
			Some(&cookie),
			Some(&csrf),
			true
		)
		.await
		.0,
		StatusCode::OK
	);
	assert_eq!(
		f.request("GET", "/htuc/", None, None, None, false).await.0,
		StatusCode::NOT_FOUND
	);
	assert_eq!(
		f.request("GET", "/hasantahsinugurcamii/2", None, None, None, false)
			.await
			.0,
		StatusCode::OK
	);
	assert_eq!(
		f.request("GET", "/hasantahsinugurcamii/3", None, None, None, false)
			.await
			.0,
		StatusCode::NOT_FOUND
	);
	let (_, data, _) = f
		.request("GET", "/veri/hasantahsinugurcamii.json", None, None, None, false)
		.await;
	assert_eq!(data["mosque"]["id"], id);
	assert_eq!(data["groups"][0]["id"], group);
	assert_eq!(before[0].id, f.app.db.readers(&group).await.unwrap()[0].id);
	assert!(f.app.config.published_dir.join(format!("{id}.json")).exists());
}
#[tokio::test]
async fn rotation_catches_up_wraps_and_rejects_stale_editor() {
	let f = Fixture::new().await;
	let (cookie, csrf) = f.login().await;
	let id = f.mosque(&cookie, &csrf, "hatim").await;
	let group = f.group(&cookie, &csrf, &id, 1).await;
	let before = f.app.db.readers(&group).await.unwrap();
	let reader = before.iter().find(|r| r.juz == 30).unwrap();
	let due = chrono::DateTime::parse_from_rfc3339("2026-09-29T21:00:00+03:00")
		.unwrap()
		.timestamp();
	sqlx::query(&f.app.db.sql("UPDATE hatim_groups SET next_due=? WHERE id=?"))
		.bind(due)
		.bind(&group)
		.execute(&f.app.db.pool)
		.await
		.unwrap();
	assert_eq!(
		f.app
			.db
			.rotate(due + 14 * 86400, chrono_tz::Europe::Istanbul)
			.await
			.unwrap(),
		1
	);
	assert_eq!(
		f.app
			.db
			.rotate(due + 14 * 86400, chrono_tz::Europe::Istanbul)
			.await
			.unwrap(),
		0
	);
	let after = f.app.db.readers(&group).await.unwrap();
	assert_eq!(after.iter().find(|r| r.id == reader.id).unwrap().juz, 3);
	assert_eq!(after.len(), 30);
	assert_eq!(
		after.iter().map(|r| r.juz).collect::<Vec<_>>(),
		(1..=30).collect::<Vec<_>>()
	);
	assert_eq!(
		f.request(
			"PUT",
			&format!("/api/groups/{group}/readers/{}", reader.id),
			Some(json!({"name":"Yanlış haftaya kayıt","version":1})),
			Some(&cookie),
			Some(&csrf),
			true
		)
		.await
		.0,
		StatusCode::CONFLICT
	);
	publication::publish(&f.app).await.unwrap();
	let file = f.app.config.published_dir.join(format!("{id}.json"));
	let time = std::fs::metadata(&file).unwrap().modified().unwrap();
	publication::publish(&f.app).await.unwrap();
	assert_eq!(time, std::fs::metadata(file).unwrap().modified().unwrap());
}
#[tokio::test]
async fn cookies_csrf_invalid_sessions_and_last_owner() {
	let f = Fixture::new().await;
	assert_eq!(
		f.request(
			"POST",
			"/api/login",
			Some(json!({"username":f.username,"password":"Yalnizca-Test-123"})),
			None,
			None,
			false
		)
		.await
		.0,
		StatusCode::FORBIDDEN
	);
	assert_eq!(
		f.request(
			"GET",
			"/api/mosques",
			None,
			Some("hatim_session=uydurma"),
			None,
			false
		)
		.await
		.0,
		StatusCode::UNAUTHORIZED
	);
	let (cookie, csrf) = f.login().await;
	assert_eq!(
		f.request(
			"POST",
			"/api/mosques",
			Some(json!({"name":"Cami","slug":"cami","schedule":Schedule::default()})),
			Some(&cookie),
			None,
			true
		)
		.await
		.0,
		StatusCode::FORBIDDEN
	);
	let (_, users, _) = f
		.request("GET", "/api/users", None, Some(&cookie), None, false)
		.await;
	let user = &users[0];
	assert_eq!(
		f.request(
			"PUT",
			&format!("/api/users/{}", user["id"].as_str().unwrap()),
			Some(
				json!({"username":f.username,"password":"","role":"manager","mosque_ids":[],"disabled":true})
			),
			Some(&cookie),
			Some(&csrf),
			true
		)
		.await
		.0,
		StatusCode::BAD_REQUEST
	);
	assert_eq!(
		f.request(
			"POST",
			"/api/logout",
			Some(json!({})),
			Some(&cookie),
			Some(&csrf),
			true
		)
		.await
		.0,
		StatusCode::OK
	);
	assert_eq!(
		f.request("GET", "/api/session", None, Some(&cookie), None, false)
			.await
			.0,
		StatusCode::UNAUTHORIZED
	);
	let production = Config::default();
	let app = App {
		db: f.app.db.clone(),
		config: production,
		publications: Default::default(),
		writes: Default::default(),
		login_limits: Default::default(),
		dummy_hash: String::new(),
	};
	let value = auth::cookie(&app, "test", false);
	assert!(value.starts_with("__Host-hatim_session="));
	assert!(value.contains("HttpOnly"));
	assert!(value.contains("SameSite=Lax"));
	assert!(value.contains("; Secure"));
}
#[tokio::test]
async fn public_routes_never_need_database_or_private_auth_data() {
	let f = Fixture::new().await;
	let (cookie, csrf) = f.login().await;
	let id = f.mosque(&cookie, &csrf, "saltokunur").await;
	f.group(&cookie, &csrf, &id, 1).await;
	f.app.db.pool.close().await;
	let (status, data, _) = f
		.request("GET", "/veri/saltokunur.json", None, None, None, false)
		.await;
	assert_eq!(status, StatusCode::OK);
	let raw = data.to_string();
	for secret in ["password_hash", "username", "totp_secret", "csrf", "token_hash"] {
		assert!(!raw.contains(secret));
	}
	assert_eq!(
		f.request("GET", "/saltokunur/1", None, None, None, false).await.0,
		StatusCode::OK
	);
}
#[tokio::test]
async fn totp_enrollment_revokes_sessions_and_rejects_replay() {
	let f = Fixture::new().await;
	let (cookie, csrf) = f.login().await;
	let (status, data, _) = f
		.request(
			"POST",
			"/api/mfa/begin",
			Some(json!({"password":"Yalnizca-Test-123","code":""})),
			Some(&cookie),
			Some(&csrf),
			true,
		)
		.await;
	assert_eq!(status, StatusCode::OK);
	let secret = data["secret"].as_str().unwrap();
	let code = auth::totp(secret, &f.username).unwrap().generate(now() as u64);
	assert_eq!(
		f.request(
			"POST",
			"/api/mfa/finish",
			Some(json!({"code":code})),
			Some(&cookie),
			Some(&csrf),
			true
		)
		.await
		.0,
		StatusCode::OK
	);
	assert_eq!(
		f.request("GET", "/api/session", None, Some(&cookie), None, false)
			.await
			.0,
		StatusCode::UNAUTHORIZED
	);
	let next_code = auth::totp(secret, &f.username)
		.unwrap()
		.generate(((now() / 30 + 1) * 30) as u64);
	let body = json!({"username":f.username,"password":"Yalnizca-Test-123","code":next_code});
	assert_eq!(
		f.request("POST", "/api/login", Some(body.clone()), None, None, true)
			.await
			.0,
		StatusCode::OK
	);
	assert_eq!(
		f.request("POST", "/api/login", Some(body), None, None, true)
			.await
			.0,
		StatusCode::UNAUTHORIZED
	);
}
#[tokio::test]
async fn account_login_is_rate_limited() {
	let f = Fixture::new().await;
	for _ in 0..10 {
		assert_eq!(
			f.request(
				"POST",
				"/api/login",
				Some(json!({"username":"unknown","password":"Yalnizca-Test-123"})),
				None,
				None,
				true
			)
			.await
			.0,
			StatusCode::UNAUTHORIZED
		);
	}
	assert_eq!(
		f.request(
			"POST",
			"/api/login",
			Some(json!({"username":"unknown","password":"Yalnizca-Test-123"})),
			None,
			None,
			true
		)
		.await
		.0,
		StatusCode::TOO_MANY_REQUESTS
	);
}
#[tokio::test]
#[ignore = "Ayrı MariaDB veya PostgreSQL test veritabanı URL'si gerekli"]
async fn external_database_round_trip() {
	let url = std::env::var("HATIM_TEST_DATABASE_URL").expect("HATIM_TEST_DATABASE_URL gerekli");
	let f = Fixture::with_url(Some(url)).await;
	let (cookie, csrf) = f.login().await;
	let id = f
		.mosque(&cookie, &csrf, &format!("test-{}", uuid::Uuid::new_v4()))
		.await;
	let group = f.group(&cookie, &csrf, &id, 1).await;
	let (_, data, _) = f
		.request(
			"GET",
			&format!("/api/groups/{group}"),
			None,
			Some(&cookie),
			None,
			false,
		)
		.await;
	assert_eq!(data["readers"].as_array().unwrap().len(), 30);
	let reader = data["readers"][0]["id"].as_str().unwrap();
	assert_eq!(
		f.request(
			"PUT",
			&format!("/api/groups/{group}/readers/{reader}"),
			Some(json!({"name":"Türkçe İsim Şükrü", "version":1})),
			Some(&cookie),
			Some(&csrf),
			true
		)
		.await
		.0,
		StatusCode::OK
	);
	assert_eq!(
		f.app.db.readers(&group).await.unwrap()[0].name,
		"Türkçe İsim Şükrü"
	);
	let manager = format!("manager-{}", uuid::Uuid::new_v4());
	assert_eq!(f.request("POST", "/api/users", Some(json!({"username":manager,"password":"Yalnizca-Test-123","role":"manager","mosque_ids":[id],"disabled":false})), Some(&cookie), Some(&csrf), true).await.0, StatusCode::OK);
	let (status, _, manager_cookie) = f
		.request(
			"POST",
			"/api/login",
			Some(json!({"username":manager,"password":"Yalnizca-Test-123"})),
			None,
			None,
			true,
		)
		.await;
	assert_eq!(status, StatusCode::OK);
	let (status, list, _) = f
		.request(
			"GET",
			"/api/mosques",
			None,
			manager_cookie.as_deref(),
			None,
			false,
		)
		.await;
	assert_eq!(status, StatusCode::OK);
	assert_eq!(list.as_array().unwrap().len(), 1);
	let due = chrono::DateTime::parse_from_rfc3339("2026-09-29T21:00:00+03:00")
		.unwrap()
		.timestamp();
	sqlx::query(&f.app.db.sql("UPDATE hatim_groups SET next_due=? WHERE id=?"))
		.bind(due)
		.bind(&group)
		.execute(&f.app.db.pool)
		.await
		.unwrap();
	assert_eq!(
		f.app
			.db
			.rotate(due + 14 * 86400, chrono_tz::Europe::Istanbul)
			.await
			.unwrap(),
		1
	);
	assert_eq!(
		f.app
			.db
			.rotate(due + 14 * 86400, chrono_tz::Europe::Istanbul)
			.await
			.unwrap(),
		0
	);
	let after = f.app.db.readers(&group).await.unwrap();
	assert_eq!(after.iter().find(|r| r.id == reader).unwrap().juz, 4);
	assert_eq!(
		after.iter().find(|r| r.id == reader).unwrap().name,
		"Türkçe İsim Şükrü"
	);

	assert_eq!(
		f.request(
			"DELETE",
			&format!("/api/mosques/{id}"),
			None,
			Some(&cookie),
			Some(&csrf),
			true
		)
		.await
		.0,
		StatusCode::OK
	);
	sqlx::query(
		&f.app
			.db
			.sql("DELETE FROM sessions WHERE user_id IN (SELECT id FROM users WHERE username=?)"),
	)
	.bind(&f.username)
	.execute(&f.app.db.pool)
	.await
	.unwrap();
	sqlx::query(&f.app.db.sql("DELETE FROM users WHERE username=?"))
		.bind(&f.username)
		.execute(&f.app.db.pool)
		.await
		.unwrap();
	sqlx::query(
		&f.app
			.db
			.sql("DELETE FROM sessions WHERE user_id IN (SELECT id FROM users WHERE username=?)"),
	)
	.bind(&manager)
	.execute(&f.app.db.pool)
	.await
	.unwrap();
	sqlx::query(&f.app.db.sql("DELETE FROM users WHERE username=?"))
		.bind(&manager)
		.execute(&f.app.db.pool)
		.await
		.unwrap();
}

#[tokio::test]
async fn user_deletion_requires_owner_and_revokes_access() {
	let f = Fixture::new().await;
	let (cookie, csrf) = f.login().await;
	let mosque = f.mosque(&cookie, &csrf, "silme-denemesi").await;
	let group = f.group(&cookie, &csrf, &mosque, 1).await;
	let before = f
		.request("GET", "/veri/silme-denemesi.json", None, None, None, false)
		.await
		.1;
	let (status, account, _) = f.request("POST", "/api/users", Some(json!({"username":"silinecek-gorevli","password":"Yalnizca-Test-123","role":"manager","mosque_ids":[mosque],"disabled":false})), Some(&cookie), Some(&csrf), true).await;
	assert_eq!(status, StatusCode::OK);
	let id = account["id"].as_str().unwrap();
	let path = format!("/api/users/{id}");
	let (status, login, manager_cookie) = f
		.request(
			"POST",
			"/api/login",
			Some(json!({"username":"silinecek-gorevli","password":"Yalnizca-Test-123"})),
			None,
			None,
			true,
		)
		.await;
	assert_eq!(status, StatusCode::OK);
	let manager_cookie = manager_cookie.unwrap();
	let manager_csrf = login["csrf"].as_str().unwrap();
	assert_eq!(
		f.request("DELETE", &path, None, None, None, true).await.0,
		StatusCode::UNAUTHORIZED
	);
	assert_eq!(
		f.request(
			"DELETE",
			&path,
			None,
			Some(&manager_cookie),
			Some(manager_csrf),
			true
		)
		.await
		.0,
		StatusCode::FORBIDDEN
	);
	assert_eq!(
		f.request("DELETE", &path, None, Some(&cookie), None, true)
			.await
			.0,
		StatusCode::FORBIDDEN
	);
	assert_eq!(
		f.request("DELETE", &path, None, Some(&cookie), Some(&csrf), false)
			.await
			.0,
		StatusCode::FORBIDDEN
	);
	assert_eq!(
		f.request(
			"DELETE",
			"/api/users/gecersiz",
			None,
			Some(&cookie),
			Some(&csrf),
			true
		)
		.await
		.0,
		StatusCode::BAD_REQUEST
	);
	let unknown = format!("/api/users/{}", uuid::Uuid::new_v4());
	assert_eq!(
		f.request("DELETE", &unknown, None, Some(&cookie), Some(&csrf), true)
			.await
			.0,
		StatusCode::NOT_FOUND
	);
	assert_eq!(
		f.request("DELETE", &path, None, Some(&cookie), Some(&csrf), true)
			.await
			.0,
		StatusCode::OK
	);
	assert_eq!(
		f.request("DELETE", &path, None, Some(&cookie), Some(&csrf), true)
			.await
			.0,
		StatusCode::NOT_FOUND
	);
	assert_eq!(
		f.request("GET", "/api/session", None, Some(&manager_cookie), None, false)
			.await
			.0,
		StatusCode::UNAUTHORIZED
	);
	assert_eq!(
		f.request(
			"POST",
			"/api/login",
			Some(json!({"username":"silinecek-gorevli","password":"Yalnizca-Test-123"})),
			None,
			None,
			true
		)
		.await
		.0,
		StatusCode::UNAUTHORIZED
	);
	for statement in [
		"SELECT COUNT(*) FROM users WHERE id=?",
		"SELECT COUNT(*) FROM sessions WHERE user_id=?",
		"SELECT COUNT(*) FROM permissions WHERE user_id=?",
	] {
		let count: i64 = sqlx::query_scalar(&f.app.db.sql(statement))
			.bind(id)
			.fetch_one(&f.app.db.pool)
			.await
			.unwrap();
		assert_eq!(count, 0);
	}
	assert_eq!(f.app.db.readers(&group).await.unwrap().len(), 30);
	assert_eq!(
		f.request("GET", "/veri/silme-denemesi.json", None, None, None, false)
			.await
			.1,
		before
	);
	assert_eq!(
		f.request("GET", "/api/session", None, Some(&cookie), None, false)
			.await
			.0,
		StatusCode::OK
	);
}

#[tokio::test]
async fn user_deletion_preserves_last_active_owner_and_clears_own_cookie() {
	let f = Fixture::new().await;
	let (cookie, csrf) = f.login().await;
	let accounts = f
		.request("GET", "/api/users", None, Some(&cookie), None, false)
		.await
		.1;
	let owner = accounts
		.as_array()
		.unwrap()
		.iter()
		.find(|u| u["username"] == f.username)
		.unwrap()["id"]
		.as_str()
		.unwrap();
	let path = format!("/api/users/{owner}");
	let (status, disabled, _) = f.request("POST", "/api/users", Some(json!({"username":"kapali-sahip","password":"Yalnizca-Test-123","role":"owner","mosque_ids":[],"disabled":true})), Some(&cookie), Some(&csrf), true).await;
	assert_eq!(status, StatusCode::OK);
	assert_eq!(
		f.request("DELETE", &path, None, Some(&cookie), Some(&csrf), true)
			.await
			.0,
		StatusCode::BAD_REQUEST
	);
	assert_eq!(
		f.request("GET", "/api/session", None, Some(&cookie), None, false)
			.await
			.0,
		StatusCode::OK
	);
	let disabled_path = format!("/api/users/{}", disabled["id"].as_str().unwrap());
	assert_eq!(
		f.request("DELETE", &disabled_path, None, Some(&cookie), Some(&csrf), true)
			.await
			.0,
		StatusCode::OK
	);
	let (status, second, _) = f.request("POST", "/api/users", Some(json!({"username":"ikinci-sahip","password":"Yalnizca-Test-123","role":"owner","mosque_ids":[],"disabled":false})), Some(&cookie), Some(&csrf), true).await;
	assert_eq!(status, StatusCode::OK);
	let (status, login, second_cookie) = f
		.request(
			"POST",
			"/api/login",
			Some(json!({"username":"ikinci-sahip","password":"Yalnizca-Test-123"})),
			None,
			None,
			true,
		)
		.await;
	assert_eq!(status, StatusCode::OK);
	let second_cookie = second_cookie.unwrap();
	let second_csrf = login["csrf"].as_str().unwrap();
	let (status, _, cleared) = f
		.request("DELETE", &path, None, Some(&cookie), Some(&csrf), true)
		.await;
	assert_eq!(status, StatusCode::OK);
	assert_eq!(cleared.as_deref(), Some("hatim_session="));
	assert_eq!(
		f.request("GET", "/api/session", None, Some(&cookie), None, false)
			.await
			.0,
		StatusCode::UNAUTHORIZED
	);
	let second_path = format!("/api/users/{}", second["id"].as_str().unwrap());
	assert_eq!(
		f.request(
			"DELETE",
			&second_path,
			None,
			Some(&second_cookie),
			Some(second_csrf),
			true
		)
		.await
		.0,
		StatusCode::BAD_REQUEST
	);
	assert_eq!(
		f.request("GET", "/api/session", None, Some(&second_cookie), None, false)
			.await
			.0,
		StatusCode::OK
	);
	let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role='owner' AND disabled=0")
		.fetch_one(&f.app.db.pool)
		.await
		.unwrap();
	assert_eq!(count, 1);
}
