# Native Entry Point

Source: `src-tauri/src/main.rs`.

- crate attribute selects the Windows GUI subsystem for non-debug builds.
- `main()` delegates all runtime construction to [`tracedoc_lib::run`](lib.rs.md).

No application logic.
