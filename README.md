# TraceDoc

TraceDoc is a local-first desktop documentation tool for Markdown workspaces. It reads folders, Markdown documents, headings, and local document links directly from the filesystem, then presents the same project model in an editor, project explorer, and architecture map. It has no account, server, database, or network requirement.

## Prerequisites

- Node.js 20.19 or newer in the Node 20 release line, or Node.js 22.12 or newer, and npm;
- Rust stable with Cargo;
- the Tauri 2 platform prerequisites for your operating system.

Install the native prerequisites from the [Tauri prerequisites guide](https://v2.tauri.app/start/prerequisites/). On Linux this includes WebKitGTK and the distribution's build tools. Windows uses Microsoft C++ Build Tools and WebView2. macOS requires Xcode Command Line Tools.

## Development

```sh
npm install
npm run tauri dev
```

`npm run dev` starts only the Vite frontend. It is useful for layout work, but filesystem commands require the Tauri application.

Validation commands:

```sh
npm run format:check
npm run lint
npm run check
npm run test:frontend
npm run check:rust
npm run lint:rust
npm run build
```

## Keyboard shortcuts

Use `Ctrl` on Windows and Linux or `Cmd` on macOS.

| Shortcut     | Action                            |
| ------------ | --------------------------------- |
| `Ctrl/Cmd+O` | Open a documentation folder       |
| `Ctrl/Cmd+S` | Save the current document         |
| `Ctrl/Cmd+F` | Find in the current document      |
| `Ctrl/Cmd+1` | Show the editor                   |
| `Ctrl/Cmd+2` | Show the architecture map         |
| `Ctrl/Cmd+0` | Show and fit the architecture map |

The project explorer width and active editor/map view are stored as disposable application preferences. TraceDoc never writes presentation metadata into a documentation workspace.

## Architecture

Rust owns workspace scanning, Markdown indexing, document I/O, and filesystem watching. Tauri commands and events carry a normalized workspace-relative project model to one Svelte store. The explorer, CodeMirror editor, link inspector, and ELK/Svelte Flow map derive their state from that store; views do not scan or parse files independently.

The main directories are:

- `src-tauri/src/services` — filesystem, document, Markdown, and watcher services;
- `src-tauri/src/commands` — the native command boundary;
- `src/lib/stores` — authoritative frontend project state;
- `src/lib/components` and `src/lib/views` — explorer, editor, inspector, and map UI;
- `src/lib/map` and `src/lib/editor` — focused layout and editing logic.

## Production builds

Build the installable packages for the current host platform:

```sh
npm ci
npm run tauri build
```

Tauri writes packages below `src-tauri/target/release/bundle`. Production packages must be built on their target operating system:

- Windows: run the command in PowerShell or a Visual Studio developer shell to create Windows installers;
- macOS: run it on macOS to create the application bundle and disk image;
- Linux: run it on a supported Linux distribution with the Tauri prerequisites to create the configured native packages.

The validation status and actually tested hosts belong in release notes. Documentation of a build path is not a claim that every platform has been tested.
