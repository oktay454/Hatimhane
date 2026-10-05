use anyhow::{Result, bail};
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Schedule {
	pub mode: String,
	pub weekday: u32,
	pub hour: u32,
	pub minute: u32,
	pub interval_days: u32,
}
impl Default for Schedule {
	fn default() -> Self {
		Self {
			mode: "weekly".into(),
			weekday: 1,
			hour: 21,
			minute: 0,
			interval_days: 7,
		}
	}
}
impl Schedule {
	pub fn validate(&self) -> Result<()> {
		if !matches!(self.mode.as_str(), "weekly" | "days")
			|| self.weekday > 6
			|| self.hour > 23
			|| self.minute > 59
			|| !(1..=365).contains(&self.interval_days)
		{
			bail!("Döngü bilgisi geçersiz");
		}
		Ok(())
	}
	fn local_time(&self, date: NaiveDate, tz: Tz) -> Result<DateTime<Tz>> {
		let naive = date.and_hms_opt(self.hour, self.minute, 0).unwrap();
		// Saatin ileri alındığı günlerde, ilk geçerli dakika seçilir.
		for minute in 0..=180 {
			if let Some(time) = tz
				.from_local_datetime(&(naive + Duration::minutes(minute)))
				.earliest()
			{
				return Ok(time);
			}
		}
		bail!("Yerel saat çözümlenemedi")
	}
	pub fn next(&self, after: i64, tz: Tz, initial: bool) -> Result<i64> {
		self.validate()?;
		let local = DateTime::from_timestamp(after, 0)
			.ok_or_else(|| anyhow::anyhow!("Tarih geçersiz"))?
			.with_timezone(&tz);
		if self.mode == "days" && !initial {
			return Ok(self
				.local_time(
					local.date_naive() + Duration::days(i64::from(self.interval_days)),
					tz,
				)?
				.timestamp());
		}
		for days in 0..=7 {
			let date = local.date_naive() + Duration::days(days);
			if self.mode == "weekly" && date.weekday().num_days_from_monday() != self.weekday {
				continue;
			}
			let due = self.local_time(date, tz)?.timestamp();
			if due > after {
				return Ok(due);
			}
		}
		bail!("Sonraki döngü hesaplanamadı")
	}
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Mosque {
	pub id: String,
	pub name: String,
	pub slug: String,
	pub schedule: Schedule,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Group {
	pub id: String,
	pub mosque_id: String,
	pub number: i64,
	pub name: String,
	pub schedule: Option<Schedule>,
	pub next_due: i64,
	pub last_rotation: i64,
	pub version: i64,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Reader {
	pub id: String,
	pub group_id: String,
	pub name: String,
	pub juz: i64,
	pub position: i64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct PublicGroup {
	pub id: String,
	pub number: i64,
	pub name: String,
	pub next_due: i64,
	pub last_rotation: i64,
	pub schedule: Schedule,
	pub readers: Vec<Reader>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Publication {
	pub mosque: Mosque,
	pub groups: Vec<PublicGroup>,
	pub generated_at: i64,
	pub timezone: String,
}
pub fn now() -> i64 {
	Utc::now().timestamp()
}
pub fn uuid(value: &str) -> bool {
	uuid::Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}
pub fn valid_slug(value: &str) -> bool {
	(2..=64).contains(&value.len())
		&& value
			.bytes()
			.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
		&& !value.starts_with('-')
		&& !value.ends_with('-')
		&& ![
			"panel", "api", "assets", "veri", "health", "favicon", "robots", "index",
		]
		.contains(&value)
}
pub fn valid_name(value: &str, max: usize) -> bool {
	!value.trim().is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn weekly_boundary_and_days() {
		let tz = chrono_tz::Europe::Istanbul;
		let before = DateTime::parse_from_rfc3339("2026-10-06T20:59:59+03:00")
			.unwrap()
			.timestamp();
		let due = before + 1;
		assert_eq!(Schedule::default().next(before, tz, true).unwrap(), due);
		assert_eq!(Schedule::default().next(due, tz, false).unwrap(), due + 7 * 86400);
		let s = Schedule {
			mode: "days".into(),
			interval_days: 3,
			..Default::default()
		};
		assert_eq!(s.next(due, tz, false).unwrap(), due + 3 * 86400);
	}
	#[test]
	fn uri_validation() {
		assert!(valid_slug("hasantahsinugurcamii"));
		assert!(valid_slug("htuc"));
		for v in ["../etc", "api", "panel", "HTUC", "cami/1", "-cami", "cami-"] {
			assert!(!valid_slug(v));
		}
	}
}
