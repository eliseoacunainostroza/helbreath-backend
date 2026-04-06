SHELL := /bin/bash

.PHONY: fmt clippy test check baseline baseline-full soak soak-record soak-trend slo-check canary-check tag-release observability-up docs-html docs-html-clean release-check backup-db restore-db restore-db-last install-admin-sudoers run-gateway run-auth run-world run-map run-chat run-admin run-jobs migrate up down seed-admin smoke smoke-full replay-fixture replay-fixture-merge replay-fixture-split replay-modern-seed replay-opcode-report capture-replay replay-fixture-synth

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

check:
	cargo check --workspace

baseline:
	bash deploy/scripts/verify_baseline.sh

baseline-full:
	RUN_FULL_STACK=1 bash deploy/scripts/verify_baseline.sh

soak:
	python3 deploy/scripts/soak_test.py --iterations $${SOAK_ITERATIONS:-3} --delay-seconds $${SOAK_DELAY_SECONDS:-3}

soak-record:
	@mkdir -p .smoke/reports
	@ts="$$(date -u +%Y%m%dT%H%M%SZ)"; \
	out=".smoke/reports/soak_$${ts}.json"; \
	echo "[soak-record] output=$$out"; \
	python3 deploy/scripts/soak_test.py --iterations $${SOAK_ITERATIONS:-3} --delay-seconds $${SOAK_DELAY_SECONDS:-3} --json-output "$$out"

soak-trend:
	python3 deploy/scripts/soak_report.py --input-glob "$${SOAK_REPORT_GLOB:-.smoke/reports/soak_*.json}" --output-md docs/soak_stability.md
	python3 deploy/scripts/generate_docs_html.py >/dev/null

slo-check:
	python3 deploy/scripts/evaluate_release_slo.py --input-glob "$${SOAK_REPORT_GLOB:-.smoke/reports/soak_*.json}" --min-iterations $${SLO_MIN_ITERATIONS:-3} --max-failed-iterations $${SLO_MAX_FAILED_ITERATIONS:-0} --min-pass-rate $${SLO_MIN_PASS_RATE:-100} --max-avg-iteration-seconds $${SLO_MAX_AVG_ITERATION_SECONDS:-30} --max-p95-iteration-seconds $${SLO_MAX_P95_ITERATION_SECONDS:-35}

canary-check:
	bash deploy/scripts/canary_check.sh

tag-release:
	bash deploy/scripts/tag_release.sh "$${VERSION:?use VERSION=v0.1.0}"

observability-up:
	bash deploy/scripts/run_observability.sh

docs-html:
	python3 deploy/scripts/generate_docs_html.py

docs-html-clean:
	rm -rf docs/html

release-check:
	bash deploy/scripts/release_gate.sh

backup-db:
	bash deploy/scripts/backup_postgres.sh

restore-db:
	bash deploy/scripts/restore_postgres.sh "$$BACKUP"

restore-db-last:
	bash deploy/scripts/restore_latest_backup.sh

install-admin-sudoers:
	bash deploy/scripts/install_admin_sudoers.sh

run-gateway:
	cargo run -p hb-gateway

run-auth:
	cargo run -p hb-auth-service

run-world:
	cargo run -p hb-world-service

run-map:
	cargo run -p hb-map-server

run-chat:
	cargo run -p hb-chat-service

run-admin:
	cargo run -p hb-admin-api

run-jobs:
	cargo run -p hb-jobs-runner

migrate:
	bash deploy/scripts/migrate.sh

seed-admin:
	bash deploy/scripts/seed_admin.sh

up:
	docker compose up -d postgres redis

down:
	docker compose down

smoke:
	python3 deploy/scripts/smoke_test.py --launch --with-db

smoke-full:
	python3 deploy/scripts/smoke_test.py --launch --full-stack --with-db

replay-fixture:
	@origin="$${REPLAY_CASE_ORIGIN:-capture}"; \
	if [ ! -f crates/net/tests/fixtures/replay_frames.bin ]; then \
		echo "[replay-fixture] sin capture raw; generando synthetic replay_frames.bin"; \
		python3 deploy/scripts/generate_replay_frames_synthetic.py --output crates/net/tests/fixtures/replay_frames.bin; \
		origin="synthetic"; \
	fi; \
	python3 deploy/scripts/replay_fixture_from_bin.py \
		--input crates/net/tests/fixtures/replay_frames.bin \
		--output crates/net/tests/fixtures/replay_cases.generated.json \
		--phase in_world \
		--protocol-version legacy_v382 \
		--expect-mode opcode_command \
		--auto-phase \
		--origin "$$origin"

replay-fixture-merge:
	python3 deploy/scripts/replay_merge_cases.py \
		--base crates/net/tests/fixtures/replay_cases.json \
		--incoming crates/net/tests/fixtures/replay_cases.generated.json \
		--output crates/net/tests/fixtures/replay_cases.json
	python3 deploy/scripts/replay_split_cases.py \
		--input crates/net/tests/fixtures/replay_cases.json \
		--legacy-output crates/net/tests/fixtures/replay_cases_legacy_v382.json \
		--modern-output crates/net/tests/fixtures/replay_cases_modern_v400.json

replay-fixture-split:
	python3 deploy/scripts/replay_split_cases.py \
		--input crates/net/tests/fixtures/replay_cases.json \
		--legacy-output crates/net/tests/fixtures/replay_cases_legacy_v382.json \
		--modern-output crates/net/tests/fixtures/replay_cases_modern_v400.json

replay-modern-seed:
	python3 deploy/scripts/replay_seed_modern_from_legacy.py \
		--input crates/net/tests/fixtures/replay_cases.json \
		--output crates/net/tests/fixtures/replay_cases.json
	python3 deploy/scripts/replay_split_cases.py \
		--input crates/net/tests/fixtures/replay_cases.json \
		--legacy-output crates/net/tests/fixtures/replay_cases_legacy_v382.json \
		--modern-output crates/net/tests/fixtures/replay_cases_modern_v400.json

replay-opcode-report:
	python3 deploy/scripts/replay_opcode_report.py \
		--input crates/net/tests/fixtures/replay_cases.json \
		--markdown-output docs/protocol_opcode_matrix.md

replay-fixture-synth:
	python3 deploy/scripts/generate_replay_frames_synthetic.py \
		--output crates/net/tests/fixtures/replay_frames.bin

capture-replay:
	python3 deploy/scripts/capture_replay_frames.py \
		--listen "$${CAPTURE_LISTEN:-127.0.0.1:3848}" \
		--upstream "$${CAPTURE_UPSTREAM:-127.0.0.1:2848}" \
		--output "$${CAPTURE_OUTPUT:-crates/net/tests/fixtures/replay_frames.bin}"
