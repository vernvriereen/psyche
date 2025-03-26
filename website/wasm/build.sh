#!/usr/bin/env bash

set -euo pipefail

echo "cleaning up prev build.."
rm -rf pkg/

echo building wasm..
wasm-pack build --target nodejs

echo building ts bindings..
cargo test export_bindings

echo fixin up ts bindings...

./fixup.sh

echo "done!"