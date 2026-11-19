#!/bin/sh
# Project-only Windows GNU cross-build environment. Does not modify shell profiles.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
BASE="$ROOT/.toolchain/windows"
HOST="$ROOT/.toolchain/root"
RUST="$BASE/rustup/toolchains/1.93.1-x86_64-unknown-linux-gnu"
MINGW="$BASE/mingw/usr"
export PATH="$RUST/bin:$MINGW/bin:$HOST/usr/bin:$PATH"
export LD_LIBRARY_PATH="$HOST/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export RUSTUP_HOME="$BASE/rustup"
export CARGO_HOME="$ROOT/.toolchain/cargo-home"
export RUSTC="$RUST/bin/rustc"
export CC="$HOST/usr/bin/gcc"
export CXX="$HOST/usr/bin/g++"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="$HOST/usr/bin/gcc"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="$MINGW/bin/x86_64-w64-mingw32-gcc-posix"
export CC_x86_64_pc_windows_gnu="$MINGW/bin/x86_64-w64-mingw32-gcc-posix"
export AR_x86_64_pc_windows_gnu="$MINGW/bin/x86_64-w64-mingw32-ar"
# Keep the official compiler's host build cache distinct from the distro compiler.
export CARGO_TARGET_DIR="$ROOT/target/windows-cross"
exec "$RUST/bin/cargo" "$@"
