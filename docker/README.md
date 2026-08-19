# TraceDoc build pipeline

Builds TraceDoc installers for Linux and Windows in Docker, without touching
the host toolchain. Requires only Docker + Docker Compose.

## What it builds

- `linux` service — native build on Ubuntu, produces `.deb`, `.rpm`,
  `.AppImage`.
- `windows` service — cross-compiles from Linux to `x86_64-pc-windows-msvc`
  via `cargo-xwin` and bundles an NSIS `.exe` installer, plus a portable
  (unbundled) `.exe`. MSI/WiX cannot be cross-compiled; it requires a real
  Windows host/VM and isn't produced here.

Output lands in `release/linux/` and `release/windows/` at the repo root.
That directory is gitignored — build artifacts sit inside the repo tree for
convenience but are never committed.

## Usage

From the repo root, `./build.sh` wraps the commands below (`./build.sh`,
`./build.sh linux`, `./build.sh windows`). Or run docker compose directly:

```sh
cd docker

# Linux packages (.deb, .rpm, .AppImage)
docker compose build linux
docker compose run --rm linux

# Windows installer (.exe NSIS + portable .exe)
docker compose build windows
docker compose run --rm windows
```

Both can run back to back; each writes only to its own `../release/<os>/`
directory. Cargo registry, npm cache, and the cargo-xwin MSVC SDK cache are
kept in named Docker volumes so repeat builds don't re-download everything.

## Publishing a GitHub release

Nothing here touches git — building and releasing are separate steps. Once a
build is verified, attach the files as release assets from the repo root:

```sh
gh release create v0.1.0-rc1 \
  release/linux/*.deb release/linux/*.rpm release/linux/*.AppImage \
  release/windows/*.exe \
  --title "v0.1.0-rc1" --notes "..."
```

`gh release create` just uploads local files as binary attachments on a git
tag — it doesn't build anything itself, so run the build first.
