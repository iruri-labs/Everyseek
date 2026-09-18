#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$PATH"
export MACOSX_DEPLOYMENT_TARGET=14.0
mkdir -p "$ROOT/.build/rust"
CORE_ARCHS="${ARCHS:-$(uname -m)}"
LIBS=()
for CORE_ARCH in $CORE_ARCHS; do
  case "$CORE_ARCH" in
    arm64) CORE_TARGET=aarch64-apple-darwin ;;
    x86_64) CORE_TARGET=x86_64-apple-darwin ;;
    *) echo "Unsupported architecture: $CORE_ARCH" >&2; exit 1 ;;
  esac
  if [[ ! -d "$(rustc --print target-libdir --target "$CORE_TARGET")" ]]; then
    if command -v rustup >/dev/null; then
      rustup target add "$CORE_TARGET"
    else
      echo "Install the Rust target $CORE_TARGET with rustup before building." >&2
      exit 1
    fi
  fi
  cargo build --manifest-path "$ROOT/Core/Cargo.toml" --target "$CORE_TARGET" --release --locked --lib
  LIBS+=("$ROOT/Core/target/$CORE_TARGET/release/libeverything_core.a")
done
lipo -create "${LIBS[@]}" -output "$ROOT/.build/rust/libeverything_core.a"
