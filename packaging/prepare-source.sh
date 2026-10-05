#!/usr/bin/env bash
set -euo pipefail

PROJE_DIZINI="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"

function KaynaklariHazirla()
{
	cd "${PROJE_DIZINI}"
	mkdir -p .cargo
	cargo vendor --locked --versioned-dirs vendor > .cargo/config.toml
}

KaynaklariHazirla
