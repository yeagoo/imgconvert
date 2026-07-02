# SPDX-License-Identifier: Apache-2.0

PNPM ?= pnpm
CARGO ?= cargo
RUST_TAURI ?= cargo +1.96.0

.PHONY: check frontend rust security test e2e tauri-smoke release-linux release-linux-debug release-linux-debug-all verify-linux-bundles linux-checksums smoke-linux-deb smoke-linux-deb-debug smoke-linux-deb-docker smoke-linux-deb-debian smoke-linux-rpm-docker smoke-linux-appimage-docker smoke-linux-docker verify-flatpak format

check: frontend rust security

frontend:
	$(PNPM) run quality:frontend

rust:
	$(PNPM) run quality:rust

security:
	$(PNPM) run quality:security

test:
	$(PNPM) run test
	$(CARGO) test -p imgconvert-core
	$(RUST_TAURI) test --manifest-path src-tauri/Cargo.toml

e2e:
	$(PNPM) run e2e

tauri-smoke:
	timeout 25s xvfb-run -a $(PNPM) tauri dev

release-linux:
	$(PNPM) run release:linux

release-linux-debug:
	$(PNPM) run release:linux:debug

release-linux-debug-all:
	$(PNPM) run release:linux:debug:all

verify-linux-bundles:
	$(PNPM) run release:linux:verify

linux-checksums:
	$(PNPM) run release:linux:checksums

smoke-linux-deb:
	$(PNPM) run release:linux:smoke:deb

smoke-linux-deb-debug:
	$(PNPM) run release:linux:smoke:deb:debug

smoke-linux-deb-docker:
	$(PNPM) run release:linux:smoke:deb:docker

smoke-linux-deb-debian:
	$(PNPM) run release:linux:smoke:deb:debian

smoke-linux-rpm-docker:
	$(PNPM) run release:linux:smoke:rpm:docker

smoke-linux-appimage-docker:
	$(PNPM) run release:linux:smoke:appimage:docker

smoke-linux-docker:
	$(PNPM) run release:linux:smoke:docker

verify-flatpak:
	$(PNPM) run release:flatpak:verify

format:
	$(PNPM) run format
	$(CARGO) fmt
	$(RUST_TAURI) fmt --manifest-path src-tauri/Cargo.toml
