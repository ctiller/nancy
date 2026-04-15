#!/bin/bash

# Copyright 2026 Craig Tiller
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

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
