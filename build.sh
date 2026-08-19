#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET="${1:-all}"

usage() {
  echo "Usage: $0 [linux|windows|all]" >&2
  echo "  SOURCE_DIR=<checkout dir name>  override which checkout to build (default: repo_release-bugfixes-2026-08-16)" >&2
  exit 1
}

build_one() {
  local service="$1"
  echo "== Building $service =="
  docker compose -f "$ROOT/docker/docker-compose.yml" build "$service"
  docker compose -f "$ROOT/docker/docker-compose.yml" run --rm "$service"
}

case "$TARGET" in
  linux|windows)
    build_one "$TARGET"
    ;;
  all)
    build_one linux
    build_one windows
    ;;
  *)
    usage
    ;;
esac

echo "Done. Artifacts in:"
echo "  $ROOT/release/linux/"
echo "  $ROOT/release/windows/"
