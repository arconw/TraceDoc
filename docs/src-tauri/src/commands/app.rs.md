# App Command

Source: `src-tauri/src/commands/app.rs`.

- `AppInfo` — serializable static name/version DTO.
- `get_app_info()` — Tauri command returning `TraceDoc` plus Cargo package version.
- `reports_the_packaged_product_identity()` — asserts runtime identity matches packaging metadata.

Frontend contract: [`types/app.ts`](../../../src/lib/types/app.ts.md).
