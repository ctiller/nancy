---
description: Trigger this workflow implicitly on EVERY Rust code change to ensure tests pass and coverage requirements are met
---
This workflow automatically enforces the testing rules defined in `.agents/rules/project.md`.
You must run this workflow at the end of your implementation cycle, before declaring a task "done" to the user.

// turbo-all
1. Run the standard check to ensure it compiles without warnings:
   `cargo check`

2. Run the test suite to ensure no integrations are broken:
   `cargo test`

3. Verify 100% test coverage mechanically for new code paths:
   `cargo llvm-cov --show-missing-lines`

4. Analyze the output of the llvm-cov terminal command. If the coverage tool lists any lines you added or modified as missing, you must immediately implement test coverage for them. Do this loop repeatedly until your code blocks are 100% covered. Paste the llvm-cov results inside your final message to present proof to the user.
