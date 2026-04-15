# ADR 0001: Use Rust and Clap for CLI

## Status

Accepted

## Context

We need to build `nancy` as a fast, stateful command-line interface. The tool needs to be distributed easily as a single binary, interact with system-level APIS reliably (like the filesystem and Git repositories), and have a robust parsing layer for subcommands, arguments, and flags.

## Decision

We will use Rust as the primary programming language for `nancy`. For argument parsing and command routing, we will use the `clap` crate (with the `derive` feature).

## Consequences

- **Positive:** Rust gives us memory safety, fast execution times, and simple single-binary distribution. 
- **Positive:** `clap` is an industry standard in the Rust ecosystem for CLI parsing, making it extremely easy to generate help menus and cleanly structure varying subcommands asynchronously.
- **Negative:** Increased initial binary size compared to a C equivalent, though acceptable for developer tooling.

<!-- IMPLEMENTED_BY: [src/main.rs] -->
