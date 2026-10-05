use crate::{config::Config, model::*};
use anyhow::{Result, bail};
use sqlx::{AnyPool, Row, any::AnyPoolOptions};

#[derive(Clone)]
pub struct Db {
	pub pool: AnyPool,
	pub postgres: bool,
	pub mysql: bool,
}
impl Db {
	pub async fn connect(config: &Config) -> Result<Self> {
		sqlx::any::install_default_drivers();
		let url = &config.database_url;
		if !["sqlite:", "mysql:", "postgres:", "postgresql:"]
			.iter()
			.any(|v| url.starts_with(v))
		{
			bail!("Veritabanı SQLite3, MariaDB veya PostgreSQL olmalı");
		}
		let pool = AnyPoolOptions::new()
			.max_connections(if url.starts_with("sqlite:") { 1 } else { 5 })
			.connect(url)
			.await?;
		let db = Self {
			pool,
			postgres: url.starts_with("postgres"),
			mysql: url.starts_with("mysql:"),
		};
		if url.starts_with("sqlite:") {
			sqlx::query("PRAGMA foreign_keys=ON").execute(&db.pool).await?;
			sqlx::query("PRAGMA busy_timeout=10000").execute(&db.pool).await?;
		}
		db.migrate().await?;
		Ok(db)
	}
	pub fn sql(&self, value: &str) -> String {
		if !self.postgres {
			return value.into();
		}
		let mut n = 0;
		value
			.chars()
			.map(|c| {
				if c == '?' {
					n += 1;
					format!("${n}")
				} else {
					c.to_string()
				}
			})
			.collect()
	}
	async fn migrate(&self) -> Result<()> {
		// VARCHAR ve BIGINT üç sürücüde de aynı UUID ve sayı temsilini kullanır.
		let statements = [
			"CREATE TABLE IF NOT EXISTS schema_version (version BIGINT PRIMARY KEY)",
			"CREATE TABLE IF NOT EXISTS users (id VARCHAR(36) PRIMARY KEY, username VARCHAR(64) NOT NULL UNIQUE, password_hash VARCHAR(512) NOT NULL, role VARCHAR(16) NOT NULL, disabled BIGINT NOT NULL DEFAULT 0, totp_secret VARCHAR(128) NOT NULL DEFAULT '', totp_pending VARCHAR(128) NOT NULL DEFAULT '', totp_last_step BIGINT NOT NULL DEFAULT -1)",
			"CREATE TABLE IF NOT EXISTS mosques (id VARCHAR(36) PRIMARY KEY, name VARCHAR(200) NOT NULL, slug VARCHAR(64) NOT NULL UNIQUE, schedule VARCHAR(2048) NOT NULL)",
			"CREATE TABLE IF NOT EXISTS hatim_groups (id VARCHAR(36) PRIMARY KEY, mosque_id VARCHAR(36) NOT NULL REFERENCES mosques(id), number BIGINT NOT NULL, name VARCHAR(200) NOT NULL, schedule VARCHAR(2048) NOT NULL, next_due BIGINT NOT NULL, last_rotation BIGINT NOT NULL, version BIGINT NOT NULL, UNIQUE(mosque_id, number))",
			"CREATE TABLE IF NOT EXISTS readers (id VARCHAR(36) PRIMARY KEY, group_id VARCHAR(36) NOT NULL REFERENCES hatim_groups(id), name VARCHAR(120) NOT NULL, juz BIGINT NOT NULL CHECK(juz BETWEEN 1 AND 30), position BIGINT NOT NULL, UNIQUE(group_id, juz))",
			"CREATE TABLE IF NOT EXISTS permissions (user_id VARCHAR(36) NOT NULL REFERENCES users(id), mosque_id VARCHAR(36) NOT NULL REFERENCES mosques(id), PRIMARY KEY(user_id,mosque_id))",
			"CREATE TABLE IF NOT EXISTS sessions (token_hash VARCHAR(64) PRIMARY KEY, user_id VARCHAR(36) NOT NULL REFERENCES users(id), csrf VARCHAR(64) NOT NULL, expires_at BIGINT NOT NULL, last_seen BIGINT NOT NULL)",
		];
		for statement in statements {
			let statement = if self.mysql {
				format!("{statement} CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci")
			} else {
				statement.to_owned()
			};
			sqlx::query(&statement).execute(&self.pool).await?;
		}
		let version: Option<i64> = sqlx::query_scalar("SELECT version FROM schema_version")
			.fetch_optional(&self.pool)
			.await?;
		if version.is_some_and(|v| v != 1) {
			bail!("Desteklenmeyen veritabanı şeması");
		}
		if version.is_none() {
			sqlx::query("INSERT INTO schema_version VALUES (1)")
				.execute(&self.pool)
				.await?;
		}
		Ok(())
	}
	pub async fn mosques(&self) -> Result<Vec<Mosque>> {
		let rows = sqlx::query("SELECT id,name,slug,schedule FROM mosques ORDER BY name,id")
			.fetch_all(&self.pool)
			.await?;
		rows.into_iter()
			.map(|r| {
				Ok(Mosque {
					id: r.try_get("id")?,
					name: r.try_get("name")?,
					slug: r.try_get("slug")?,
					schedule: serde_json::from_str(r.try_get::<&str, _>("schedule")?)?,
				})
			})
			.collect()
	}
	pub async fn groups(&self, mosque_id: &str) -> Result<Vec<Group>> {
		let rows=sqlx::query(&self.sql("SELECT id,mosque_id,number,name,schedule,next_due,last_rotation,version FROM hatim_groups WHERE mosque_id=? ORDER BY number")).bind(mosque_id).fetch_all(&self.pool).await?;
		rows.into_iter()
			.map(|r| {
				let s: &str = r.try_get("schedule")?;
				Ok(Group {
					id: r.try_get("id")?,
					mosque_id: r.try_get("mosque_id")?,
					number: r.try_get("number")?,
					name: r.try_get("name")?,
					schedule: if s.is_empty() {
						None
					} else {
						Some(serde_json::from_str(s)?)
					},
					next_due: r.try_get("next_due")?,
					last_rotation: r.try_get("last_rotation")?,
					version: r.try_get("version")?,
				})
			})
			.collect()
	}
	pub async fn readers(&self, group_id: &str) -> Result<Vec<Reader>> {
		let rows = sqlx::query(
			&self.sql("SELECT id,group_id,name,juz,position FROM readers WHERE group_id=? ORDER BY juz"),
		)
		.bind(group_id)
		.fetch_all(&self.pool)
		.await?;
		rows.into_iter()
			.map(|r| {
				Ok(Reader {
					id: r.try_get("id")?,
					group_id: r.try_get("group_id")?,
					name: r.try_get("name")?,
					juz: r.try_get("juz")?,
					position: r.try_get("position")?,
				})
			})
			.collect()
	}
	pub async fn allowed(&self, user_id: &str, role: &str, mosque_id: &str) -> Result<bool> {
		if role == "owner" {
			return Ok(true);
		}
		let n: i64 =
			sqlx::query_scalar(&self.sql("SELECT COUNT(*) FROM permissions WHERE user_id=? AND mosque_id=?"))
				.bind(user_id)
				.bind(mosque_id)
				.fetch_one(&self.pool)
				.await?;
		Ok(n > 0)
	}
	pub async fn rotate(&self, at: i64, tz: chrono_tz::Tz) -> Result<usize> {
		let mut changed = 0;
		for mosque in self.mosques().await? {
			for group in self.groups(&mosque.id).await? {
				if group.next_due > at {
					continue;
				}
				let schedule = group.schedule.as_ref().unwrap_or(&mosque.schedule);
				let (mut due, mut last, mut count) = (group.next_due, group.last_rotation, 0_i64);
				while due <= at {
					last = due;
					due = schedule.next(due, tz, false)?;
					count += 1;
					if count > 200000 {
						bail!("Döngü tarihi aşırı eski");
					}
				}
				let readers = self.readers(&group.id).await?;
				let mut tx = self.pool.begin().await?;
				let result = sqlx::query(&self.sql(
					"UPDATE hatim_groups SET next_due=?,last_rotation=?,version=version+1 WHERE id=? AND version=?",
				))
				.bind(due)
				.bind(last)
				.bind(&group.id)
				.bind(group.version)
				.execute(&mut *tx)
				.await?;
				if result.rows_affected() != 1 {
					tx.rollback().await?;
					continue;
				}
				sqlx::query(&self.sql("DELETE FROM readers WHERE group_id=?"))
					.bind(&group.id)
					.execute(&mut *tx)
					.await?;
				for r in readers {
					sqlx::query(
						&self.sql("INSERT INTO readers (id,group_id,name,juz,position) VALUES (?,?,?,?,?)"),
					)
					.bind(r.id)
					.bind(r.group_id)
					.bind(r.name)
					.bind((r.juz - 1 + count) % 30 + 1)
					.bind(r.position)
					.execute(&mut *tx)
					.await?;
				}
				tx.commit().await?;
				changed += 1;
			}
		}
		Ok(changed)
	}
}
