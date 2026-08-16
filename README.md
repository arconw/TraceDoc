# TraceDoc

TraceDoc is a local-first desktop documentation tool. It opens a local folder and builds a Markdown-oriented project model from its directory structure.

## Markdown indexing

Document titles come from the first non-empty H1, or from the filename without `.md` when an H1 is absent. The index records all headings and outgoing ordinary Markdown links. Links with `http`, `https`, `mailto`, another URI scheme, or a protocol-relative URL are excluded from the project graph. Heading fragments resolve to the target document, and a fragment without a path resolves to the source document.

Simple wiki links use `[[target]]`, `[[folder/target]]`, and optional `.md` extensions. Path-shaped wiki targets resolve relative to the source document. A bare target prefers an exact filename or stem in the source folder, then resolves only when that filename or stem is unique across the workspace. Ambiguous and missing targets remain unresolved with a diagnostic reason. Piped aliases such as `[[target|label]]` are intentionally unsupported.

## Development

Install dependencies with `npm install`, then use:

- `npm run tauri dev` to launch the desktop application;
- `npm run dev` to run only the frontend development server;
- `npm run check` to run Svelte and TypeScript checks;
- `npm run test:frontend` to run frontend state regression tests;
- `npm run check:rust` to check the Rust application;
- `npm run lint` to lint frontend sources;
- `npm run lint:rust` to lint Rust sources;
- `npm run format:check` to verify formatting;
- `npm run build` to build the frontend;
- `npm run tauri build` to create a production desktop build.
