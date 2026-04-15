---
name: Codebase Xlink Microformat Maintenance
description: Rules for correctly using and resolving codebase-wide linked references between docs, tests, and sources: IMPLEMENTED_BY, DOCUMENTED_BY, TESTED_BY, DEPRECATES, DEPRECATED_BY, SEE_ALSO.
---

# Codebase Xlink Microformat Maintenance

The project utilizes a static string microformat mapping feature implementations to their original architecture, documentation, and automated tests.

When orchestrating logic or modifying systems, you must accurately maintain these mappings across the files you touch.

## Formats

When editing source files, mark what document tracks it and what tests exercise it:
```rust
// DOCUMENTED_BY: [docs/adr/0010-feature.md]
// TESTED_BY: [tests/my_feature_integration_logs.rs]
// SEE_ALSO: [src/other/module.rs]
```

When authoring testing suites, bind them to the specific module they cover:
```rust
// IMPLEMENTED_BY: [src/feature/module.rs]
```

When appending docs/ADRs, point them down to the implementation:
```markdown
<!-- IMPLEMENTED_BY: [src/feature/module.rs] -->
```

## Mandatory Rules

1. Every source file must have at least one `DOCUMENTED_BY: [...]` tag linking out.
2. Every documentation file must contain an `IMPLEMENTED_BY: [...]` tag. If a documentation file is project-wide and has no specific implemented code mappings, it must map to `[none]`.
3. If an implementation source is mapping it's test suite via `TESTED_BY: [...]`, that test suite file must correctly point back with an `IMPLEMENTED_BY`.
4. Extraneous comments are ignored (e.g. `//`, `<!--`, `*`), but `[` and `]` must encapsulate the list. Comma-separated multi-line bounds are allowed.

## Utility Commands

When appending links, you can use the built in utilities across your workspace organically:
- `nancy xlink audit` - Verifies no broken or unilateral mapping exists across tracked git files natively safely.
- `nancy xlink add-implemented-by docs/adr/0010-test.md src/test.rs` - Adds the bidirectional connections automatically.
- `nancy xlink add-documented-by src/test.rs docs/adr/0010-test.md`
