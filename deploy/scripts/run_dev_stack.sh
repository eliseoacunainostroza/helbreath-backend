#!/usr/bin/env bash
set -euo pipefail

# Starts core services in development terminals using cargo.
# Run this script from repository root in one shell, then open another shell for logs.

cargo run -p hb-admin-api &
PID_ADMIN=$!

cargo run -p hb-auth-service &
PID_AUTH=$!

cargo run -p hb-gateway &
PID_GATEWAY=$!

cargo run -p hb-world-service &
PID_WORLD=$!

cargo run -p hb-map-server &
PID_MAP=$!

cargo run -p hb-chat-service &
PID_CHAT=$!

cargo run -p hb-jobs-runner &
PID_JOBS=$!

trap 'kill ${PID_ADMIN} ${PID_AUTH} ${PID_GATEWAY} ${PID_WORLD} ${PID_MAP} ${PID_CHAT} ${PID_JOBS} 2>/dev/null || true' EXIT
wait
