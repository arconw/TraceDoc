#!/usr/bin/env bash
set -euo pipefail

cd /workspace

npm ci
npm run tauri build -- --bundles deb,rpm,appimage

mkdir -p /out
find src-tauri/target/release/bundle -maxdepth 2 -type f \
  \( -name '*.deb' -o -name '*.rpm' -o -name '*.AppImage' \) \
  -exec cp -v {} /out/ \;

chown "$(stat -c '%u:%g' /out)" /out/*
