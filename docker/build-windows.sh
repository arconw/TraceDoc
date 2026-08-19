#!/usr/bin/env bash
set -euo pipefail

cd /workspace

npm ci
npm run tauri build -- --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis

mkdir -p /out
find src-tauri/target/x86_64-pc-windows-msvc/release/bundle -maxdepth 2 -type f \
  -name '*.exe' \
  -exec cp -v {} /out/ \;

cp -v src-tauri/target/x86_64-pc-windows-msvc/release/tracedoc.exe \
  /out/TraceDoc_0.1.0_portable.exe

chown "$(stat -c '%u:%g' /out)" /out/*
