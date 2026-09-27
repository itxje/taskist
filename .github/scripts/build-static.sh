#!/usr/bin/env bash
# Build a static `tk` for a musl target on a runner of the same architecture,
# check that it is statically linked, and run it once.
set -euo pipefail

target="$1"
cc_var="CC_${target//-/_}"
export "$cc_var=musl-gcc"

cargo build --locked --profile dist --target "$target"

bin="target/$target/dist/tk"
if readelf --program-headers "$bin" | grep -q 'Requesting program interpreter' \
  || readelf --dynamic "$bin" | grep -q '(NEEDED)'; then
  echo "error: $bin is not statically linked" >&2
  readelf --program-headers --dynamic "$bin" >&2
  exit 1
fi
"$bin" --version
