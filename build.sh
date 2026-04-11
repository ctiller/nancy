#!/bin/bash
set -ex

# Statically strictly order the build pipeline to prevent cargo pipelining race conditions
# where the backend `rust-embed` (or `include_bytes!`) macros natively evaluate before
# the frontend WASM bundle finishes generating.

# 1. First, build the frontend bundle natively into dist via Trunk
cd web
trunk build --release
cd ..

# 2. Safely synchronize web/dist into an isolated asset persistence directory
# We use rsync with checksums (-c) to guarantee destination timestamps are only updated 
# if the frontend WASM genuinely changed, preventing useless backend recompilations.
mkdir -p src/web/site
rsync -ac --delete web/dist/ src/web/site/

# 3. Third, build the server securely after the assets strictly exist in the custom persistence layer
cargo build --release -p nancy
