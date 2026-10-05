use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use hatimhane::{App, auth, config::Config, db::Db, model::*, publication, routes};
use sqlx::Row;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "hatimhane", version, about = "Çoklu cami ve hatim grubu yönetimi")]
struct Cli {
	#[arg(long, default_value = "/etc/hatimhane.d", global = true)]
	config_dir: PathBuf,
	#[command(subcommand)]
	command: Command,
}
#[derive(Subcommand)]
enum Command {
	Serve,
	Owner {
		#[arg(long)]
		username: String,
		#[arg(long)]
		password_file: Option<PathBuf>,
		#[arg(long)]
		reset: bool,
	},
	Import {
		#[arg(long)]
		json: PathBuf,
		#[arg(long)]
		slug: String,
		#[arg(long)]
		mosque_name: Option<String>,
	},
	Check,
}
#[tokio::main]
async fn main() -> Result<()> {
	tracing_subscriber::fmt()
		.with_env_filter(
			tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "hatimhane=info".into()),
		)
		.init();
	let cli = Cli::parse();
	let config = Config::load(&cli.config_dir)?;
	match cli.command {
		Command::Check => {
			let db = Db::connect(&config).await?;
			db.pool.close().await;
			println!("Yapılandırma ve veritabanı hazır.");
		}
		Command::Owner {
			username,
			password_file,
			reset,
		} => {
			if !(3..=64).contains(&username.len())
				|| !username
					.bytes()
					.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || [b'-', b'_', b'.'].contains(&b))
			{
				bail!("Kullanıcı adı geçersiz");
			}
			let password = if let Some(file) = password_file {
				std::fs::read_to_string(file)?
					.trim_end_matches(['\n', '\r'])
					.to_owned()
			} else {
				let a = rpassword::prompt_password("Parola (en az 12 karakter): ")?;
				let b = rpassword::prompt_password("Parola tekrar: ")?;
				if a != b {
					bail!("Parolalar eşleşmedi");
				}
				a
			};
			let hash = auth::hash_password(&password)?;
			let db = Db::connect(&config).await?;
			let old = sqlx::query(&db.sql("SELECT id FROM users WHERE username=?"))
				.bind(&username)
				.fetch_optional(&db.pool)
				.await?;
			if let Some(old) = old {
				if !reset {
					bail!("Hesap zaten var; kurtarma için açıkça --reset kullanın");
				}
				let id: String = old.try_get("id")?;
				let mut tx = db.pool.begin().await?;
				sqlx::query(&db.sql("UPDATE users SET password_hash=?,role='owner',disabled=0,totp_secret='',totp_pending='',totp_last_step=-1 WHERE id=?")).bind(hash).bind(&id).execute(&mut *tx).await?;
				sqlx::query(&db.sql("DELETE FROM sessions WHERE user_id=?"))
					.bind(id)
					.execute(&mut *tx)
					.await?;
				tx.commit().await?;
			} else {
				sqlx::query(
					&db.sql("INSERT INTO users (id,username,password_hash,role) VALUES (?,?,?,'owner')"),
				)
				.bind(uuid::Uuid::new_v4().to_string())
				.bind(username)
				.bind(hash)
				.execute(&db.pool)
				.await?;
			}
			println!("Sunucu sahibi hesabı hazır. Parola yazdırılmadı.");
		}
		Command::Import {
			json,
			slug,
			mosque_name,
		} => {
			if !valid_slug(&slug) {
				bail!("Cami adresi geçersiz");
			}
			let value: serde_json::Value = serde_json::from_slice(&std::fs::read(json)?)?;
			let name = mosque_name
				.or_else(|| value["cami_adi"].as_str().map(str::to_owned))
				.ok_or_else(|| anyhow::anyhow!("Cami adı gerekli"))?;
			if !valid_name(&name, 200) {
				bail!("Cami adı geçersiz");
			}
			let start = chrono::DateTime::parse_from_rfc3339(
				value["hafta_baslangici"]
					.as_str()
					.ok_or_else(|| anyhow::anyhow!("Hafta başlangıcı gerekli"))?,
			)?
			.timestamp();
			let list = value["katilimcilar"]
				.as_array()
				.ok_or_else(|| anyhow::anyhow!("Katılımcılar gerekli"))?;
			let mut seen = std::collections::HashSet::new();
			let mut readers = Vec::new();
			for (index, reader) in list.iter().enumerate() {
				let name = reader["ad"]
					.as_str()
					.ok_or_else(|| anyhow::anyhow!("İsim gerekli"))?;
				let juz = reader["cuz"]
					.as_i64()
					.ok_or_else(|| anyhow::anyhow!("Cüz gerekli"))?;
				if !valid_name(name, 120) || !(1..=30).contains(&juz) || !seen.insert(juz) {
					bail!("İsim/cüz geçersiz veya cüz tekrarlanmış");
				}
				readers.push((name.to_owned(), juz, index as i64));
			}
			let db = Db::connect(&config).await?;
			let schedule = Schedule::default();
			let due = schedule.next(start, config.timezone.parse()?, false)?;
			let mosque_id = uuid::Uuid::new_v4().to_string();
			let group_id = uuid::Uuid::new_v4().to_string();
			let mut tx = db.pool.begin().await?;
			sqlx::query(&db.sql("INSERT INTO mosques (id,name,slug,schedule) VALUES (?,?,?,?)"))
				.bind(&mosque_id)
				.bind(name)
				.bind(slug)
				.bind(serde_json::to_string(&schedule)?)
				.execute(&mut *tx)
				.await?;
			sqlx::query(&db.sql("INSERT INTO hatim_groups (id,mosque_id,number,name,schedule,next_due,last_rotation,version) VALUES (?,?,1,'Hatim Grubu','',?,?,1)")).bind(&group_id).bind(&mosque_id).bind(due).bind(start).execute(&mut *tx).await?;
			for (name, juz, position) in readers {
				sqlx::query(
					&db.sql("INSERT INTO readers (id,group_id,name,juz,position) VALUES (?,?,?,?,?)"),
				)
				.bind(uuid::Uuid::new_v4().to_string())
				.bind(&group_id)
				.bind(name)
				.bind(juz)
				.bind(position)
				.execute(&mut *tx)
				.await?;
			}
			tx.commit().await?;
			println!(
				"JSON içe aktarıldı. Cami UUID: {mosque_id}; grup UUID: {group_id}. Hizmet başladığında yayınlanır."
			);
		}
		Command::Serve => {
			let app = App::new(config).await?;
			let scheduler = app.clone();
			tokio::spawn(async move {
				let mut interval =
					tokio::time::interval(std::time::Duration::from_secs(scheduler.config.scheduler_seconds));
				loop {
					interval.tick().await;
					let _guard = scheduler.writes.lock().await;
					if let Err(e) = scheduler
						.db
						.rotate(now(), scheduler.config.timezone.parse().unwrap())
						.await
					{
						tracing::error!("Döngü güncellenemedi: {e}");
					}
					if let Err(e) = publication::publish(&scheduler).await {
						tracing::error!("JSON yayımlanamadı: {e}");
					}
					if let Err(e) = sqlx::query(
						&scheduler
							.db
							.sql("DELETE FROM sessions WHERE expires_at<=? OR last_seen<=?"),
					)
					.bind(now())
					.bind(now() - scheduler.config.idle_minutes * 60)
					.execute(&scheduler.db.pool)
					.await
					{
						tracing::warn!("Oturum temizliği başarısız: {e}");
					}
				}
			});
			let listener = tokio::net::TcpListener::bind(app.config.listen).await?;
			tracing::info!(listen=%app.config.listen,"Hatimhane hazır");
			axum::serve(
				listener,
				routes::router(app).into_make_service_with_connect_info::<std::net::SocketAddr>(),
			)
			.with_graceful_shutdown(shutdown())
			.await?;
		}
	}
	Ok(())
}
async fn shutdown() {
	let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
	tokio::select! {_=tokio::signal::ctrl_c()=>{},_=terminate.recv()=>{}}
}
