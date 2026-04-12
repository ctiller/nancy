---
name: Compile-Time Markdown Serialization
description: Instruction on creating type-safe serialization mapping for Markdown string payloads gracefully.
---

# Compile-Time Markdown Serialization

Nancy uses custom procedural macros (the `llm-macros` crate) to parse unstructured Markdown string outputs returned by Gemini.

## Guidelines for Modifying Parsers

1. **Procedural Macro Parsers**: Do not use raw regex strings to dynamically scrape massive LLM responses. Define your expected fields on standard Rust structs and use the `llm-macros` procedural wrappers to safely extract the data.
2. **Schema Alignment Constraints**: Struct properties defined within these macros intrinsically guide the LLM prompts. This ensures the output matches our expected structure and eliminates the need for complex manual fallback parsing.
