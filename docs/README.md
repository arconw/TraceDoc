# TraceDoc Code Documentation

This tree mirrors code only:

- [`src`](src/README.md) — Svelte/TypeScript frontend.
- [`src-tauri/src`](src-tauri/src/README.md) — Rust/Tauri backend.
- [`SHARED.md`](SHARED.md) — reusable cross-feature API registry.

Build configuration, package manifests, generated output, icons, and other non-code files are intentionally excluded.

## Mapping rule

For source file `X/Y/name.ext`, maintain `docs/X/Y/name.ext.md`. Every mirrored directory contains `README.md` with:

- directory responsibility and boundary;
- links to every child directory and documented source file;
- upstream inputs and downstream consumers;
- invariants that apply to the whole directory.

Every leaf document contains, tersely:

- responsibility;
- inputs, outputs, state, events, and side effects;
- dependencies and consumers as documentation links;
- every function, method, command, reducer, component callback, and important type;
- test intent for test files;
- platform or lifecycle invariants where relevant.

## Update procedure

1. Read this file, [`SHARED.md`](SHARED.md), and the relevant directory indexes.
2. Read the leaf documents before reading or changing implementation.
3. Reuse an API from the shared registry when its contract fits; do not duplicate it inside a feature.
4. Change code and its leaf documentation together.
5. Update parent indexes if ownership or dependencies changed.
6. Update the shared registry when cross-feature reuse changed.
7. Verify that each code file has exactly one mirrored leaf document and every internal documentation link resolves.

Source comments are intentionally avoided; durable technical context belongs here.
