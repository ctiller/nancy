# Compile-Time Markdown Serialization

## Title
Implement compile-time Markdown frontmatter serialization via native procedural macros.

## Context
As Nancy expands its documentation and static data surfaces, we needed an explicit way to load a lot of `.md` files equipped with YAML frontmatter natively into the application codebase. A primary constraint for Nancy is distribution: we want to distribute the application as a single native binary without any ancillary data requirements (no external documentation or manifest folders required to boot the tool). Because dynamic runtime parsing disables robust `const` or `static` declarations, we needed a macro-bound serialization approach to execute at compile time.

## Decision
We implemented a pair of procedural macros within the `llm-macros` crate to natively deserialize `.md` file payloads explicitly into heavily optimized rust `const` structures.

- **`#[md_defined]` Attribute Macro**: Parses the structural schema for a document natively, converting declared dynamic `string` fields down to strict `&'static str` fields cleanly at compile time and pruning the `#[body]` layout attribute. This strictly empowers `const` compatibility entirely behind the scenes structure-wise.
- **`include_md!` Macro**: Implements a dedicated file loader parameterization `include_md!(StructType, "filename.md")`. This macro executes relative to the `CARGO_MANIFEST_DIR` across builds, splitting the simplistic YAML frontmatter `key: value` mappings before the `---` border seamlessly onto struct fields and allocating the remaining generic string explicitly to the parsed `body` value statically. 

## Consequences
- Nancy can seamlessly pack and distribute extensive `.md` data arrays statically as one optimized native binary artifact without file I/O runtime penalties.
- Data struct fields designated `String` or `string` in `md_defined` struct schemas securely mutate to `&'static str` types, cementing efficient compilation restrictions across data definitions locally. 
- Due to early-expansion constraints in rust AST mapping, calling the procedural parser requires explicitly dictating the matching StructType alongside the filepath (e.g., `include_md!(Great, ...)`).
- We utilize `serde` and `serde_yaml` as local build dependencies inside the `llm-macros` procedural parsing pipeline to achieve robust, nested, and multiline frontmatter YAML extraction seamlessly without penalizing the final application's production runtime binary size.
