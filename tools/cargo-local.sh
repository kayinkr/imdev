#!/bin/sh
# Rootless Linux-only build helper. Normal macOS/Windows builds use cargo directly.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
PREFIX="$ROOT/.toolchain/root"
export PATH="$PREFIX/usr/bin:$PATH"
export LD_LIBRARY_PATH="$PREFIX/usr/lib/x86_64-linux-gnu:$PREFIX/usr/lib/rust-1.93/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export CARGO_HOME="$ROOT/.toolchain/cargo-home"
export CC="$PREFIX/usr/bin/gcc"
export CXX="$PREFIX/usr/bin/g++"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="$PREFIX/usr/bin/gcc"
exec "$PREFIX/usr/bin/cargo" "$@"
