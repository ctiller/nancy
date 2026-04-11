#!/bin/bash
set -ex

./build.sh
cd .scratch
RUST_LOG=info,tower_http=debug ../target/release/nancy coordinator --port 3000
