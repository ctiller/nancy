#!/bin/bash

cargo leptos build
cd .scratch
../target/debug/nancy coordinator --port 3000
