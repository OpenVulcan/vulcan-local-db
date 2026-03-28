#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"

cd "${ROOT_DIR}"

protoc \
  -I "${ROOT_DIR}" \
  --go_out="${SCRIPT_DIR}" \
  --go_opt=paths=source_relative \
  --go-grpc_out="${SCRIPT_DIR}" \
  --go-grpc_opt=paths=source_relative \
  "proto/v1/duckdb.proto"

echo "generated Go stubs under ${SCRIPT_DIR}/proto/v1"
