#!/usr/bin/env python3
"""Cargo.lock içindeki kütüphanelerin lisans bildirimlerini pakete ekler."""
import json
import os
from pathlib import Path
import sys
import tomllib

output = Path(sys.argv[1])
output.mkdir(parents=True, exist_ok=True)
root = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo"))) / "registry/src"
registries = list(root.glob("*"))
vendor = Path("vendor")
metadata = []
for entry in tomllib.loads(Path("Cargo.lock").read_text())["package"]:
	if "source" not in entry:
		continue
	name = f'{entry["name"]}-{entry["version"]}'
	candidates = [p / name for p in registries] + [vendor / entry["name"], vendor / name]
	crate = next((p for p in candidates if (p / "Cargo.toml").exists()), None)
	if crate is None:
		raise RuntimeError(f"Lisans kaynağı eksik: {name}")
	manifest = tomllib.loads((crate / "Cargo.toml").read_text())["package"]
	files = [p for p in crate.rglob("*") if p.is_file() and p.name.upper().startswith(("LICENSE", "COPYING", "NOTICE"))]
	folder = output / name
	folder.mkdir(exist_ok=True)
	for source in files:
		target = folder / source.relative_to(crate)
		target.parent.mkdir(parents=True, exist_ok=True)
		target.write_bytes(source.read_bytes())
	metadata.append({"crate": name, "license": manifest.get("license", ""), "repository": manifest.get("repository", ""), "notice_files": [str(p.relative_to(crate)) for p in files]})
(output / "index.json").write_text(json.dumps(metadata, ensure_ascii=False, indent="\t") + "\n")
