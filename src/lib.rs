pub mod auth;
pub mod config;
pub mod db;
pub mod model;
pub mod publication;
pub mod routes;

use axum::{
	Json,
	http::StatusCode,
	response::{IntoResponse, Response},
};
use config::Config;
use db::Db;
use model::Publication;
use serde_json::json;
use std::{
	collections::{BTreeMap, HashMap},
	sync::Arc,
};
use tokio::sync::{Mutex, RwLock};

pub struct App {
	pub db: Db,
	pub config: Config,
	pub publications: RwLock<BTreeMap<String, Publication>>,
	pub writes: Mutex<()>,
	pub login_limits: Mutex<HashMap<String, (i64, u32)>>,
	pub dummy_hash: String,
}
impl App {
	pub async fn new(config: Config) -> anyhow::Result<Arc<Self>> {
		config.validate()?;
		std::fs::create_dir_all(&config.published_dir)?;
		let db = Db::connect(&config).await?;
		let app = Arc::new(Self {
			db,
			config,
			publications: RwLock::new(BTreeMap::new()),
			writes: Mutex::new(()),
			login_limits: Mutex::new(HashMap::new()),
			dummy_hash: auth::hash_password(&auth::token())?,
		});
		app.db.rotate(model::now(), app.config.timezone.parse()?).await?;
		publication::publish(&app).await?;
		Ok(app)
	}
}
#[derive(Debug)]
pub struct AppError {
	pub status: StatusCode,
	pub message: String,
}
pub type AppResult<T> = Result<T, AppError>;
impl AppError {
	pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
		Self {
			status,
			message: message.into(),
		}
	}
	pub fn bad(message: impl Into<String>) -> Self {
		Self::new(StatusCode::BAD_REQUEST, message)
	}
	pub fn forbidden() -> Self {
		Self::new(StatusCode::FORBIDDEN, "Bu işlem için yetkiniz yok.")
	}
	pub fn unauthorized() -> Self {
		Self::new(StatusCode::UNAUTHORIZED, "Giriş yapmanız gerekiyor.")
	}
	pub fn missing() -> Self {
		Self::new(StatusCode::NOT_FOUND, "Kayıt bulunamadı.")
	}
	pub fn stale() -> Self {
		Self::new(
			StatusCode::CONFLICT,
			"Liste değişti. Listeyi yenileyip cüzü kontrol edin.",
		)
	}
}
impl From<sqlx::Error> for AppError {
	fn from(e: sqlx::Error) -> Self {
		if let sqlx::Error::Database(ref db) = e {
			if db.is_unique_violation() {
				return Self::new(
					StatusCode::CONFLICT,
					"Adres veya numara zaten kullanımda. Başka bir değer seçin.",
				);
			}
		}
		tracing::error!(error_type=?std::mem::discriminant(&e),"Veritabanı işlemi başarısız");
		Self::new(
			StatusCode::INTERNAL_SERVER_ERROR,
			"İşlem tamamlanamadı. Yeniden deneyin.",
		)
	}
}
impl From<anyhow::Error> for AppError {
	fn from(e: anyhow::Error) -> Self {
		tracing::error!("Uygulama işlemi başarısız: {e}");
		Self::new(
			StatusCode::INTERNAL_SERVER_ERROR,
			"İşlem tamamlanamadı. Yeniden deneyin.",
		)
	}
}
impl IntoResponse for AppError {
	fn into_response(self) -> Response {
		(self.status, Json(json!({"error":self.message}))).into_response()
	}
}
