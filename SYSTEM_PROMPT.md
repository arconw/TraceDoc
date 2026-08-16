# TraceDoc Engineering Rules

Before changing code:

1. Read `docs/README.md`.
2. Read the mirrored documentation for every source directory and file in scope.
3. Check `docs/SHARED.md` and reuse an existing shared component, type, state transition, or helper when it satisfies the requirement.

For every code change:

- Update the corresponding mirrored documentation in the same change.
- Update directory indexes when files, responsibilities, dependencies, or public boundaries change.
- Update `docs/SHARED.md` when a reusable API is added, removed, renamed, or changes semantics.
- Do not add comments to source code. Put design intent, invariants, lifecycle rules, platform behavior, and non-obvious decisions in documentation.
- Preserve feature ownership: feature-specific code stays in its feature; only code with real cross-feature consumers belongs in shared modules.
- Keep commits short, semantic, and focused. Commit messages may reference a documentation section when useful.

Documentation is part of the implementation contract. A code change with stale documentation is incomplete.
