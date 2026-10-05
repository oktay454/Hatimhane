use crate::{App, model::*};
use anyhow::Result;
use std::{collections::BTreeMap, io::Write, path::Path};

fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
	let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
	let mut file = std::fs::OpenOptions::new()
		.write(true)
		.create_new(true)
		.open(&temp)?;
	let result = (|| {
		file.write_all(bytes)?;
		file.sync_all()?;
		std::fs::rename(&temp, path)?;
		Ok(())
	})();
	if result.is_err() {
		let _ = std::fs::remove_file(&temp);
	}
	result
}
pub async fn publish(app: &App) -> Result<()> {
	let mut current = BTreeMap::new();
	for mosque in app.db.mosques().await? {
		let mut groups = Vec::new();
		for group in app.db.groups(&mosque.id).await? {
			groups.push(PublicGroup {
				id: group.id.clone(),
				number: group.number,
				name: group.name,
				next_due: group.next_due,
				last_rotation: group.last_rotation,
				schedule: group.schedule.unwrap_or_else(|| mosque.schedule.clone()),
				readers: app.db.readers(&group.id).await?,
			});
		}
		let publication = Publication {
			mosque: mosque.clone(),
			groups,
			generated_at: now(),
			timezone: app.config.timezone.clone(),
		};
		// Hesap, parola, oturum ve yetki verileri bu yapıda bulunmaz.
		let path = app.config.published_dir.join(format!("{}.json", mosque.id));
		let mut value = serde_json::to_value(&publication)?;
		// Değişiklik yoksa dosyaya yeniden yazılmaz.
		let changed = std::fs::read(&path)
			.ok()
			.and_then(|v| serde_json::from_slice::<serde_json::Value>(&v).ok())
			.map(|mut old| {
				old["generated_at"] = value["generated_at"].clone();
				old != value
			})
			.unwrap_or(true);
		if changed {
			atomic(&path, &serde_json::to_vec_pretty(&publication)?)?;
		} else if let Ok(old) = std::fs::read(&path) {
			if let Ok(old) = serde_json::from_slice::<Publication>(&old) {
				value["generated_at"] = old.generated_at.into();
			}
		}
		current.insert(mosque.slug.clone(), serde_json::from_value::<Publication>(value)?);
	}
	let manifest: BTreeMap<_, _> = current
		.iter()
		.map(|(slug, p)| (slug.clone(), p.mosque.id.clone()))
		.collect();
	let manifest_path = app.config.published_dir.join("index.json");
	let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
	if std::fs::read(&manifest_path).ok().as_deref() != Some(manifest_bytes.as_slice()) {
		atomic(&manifest_path, &manifest_bytes)?;
	}
	for file in std::fs::read_dir(&app.config.published_dir)? {
		let path = file?.path();
		if path.extension().is_some_and(|v| v == "json")
			&& path
				.file_stem()
				.and_then(|v| v.to_str())
				.is_some_and(|id| uuid(id) && !manifest.values().any(|v| v == id))
		{
			std::fs::remove_file(path)?;
		}
	}
	*app.publications.write().await = current;
	Ok(())
}
