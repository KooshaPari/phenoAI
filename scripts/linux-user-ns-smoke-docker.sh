#!/usr/bin/env bash
# Run USER ns live smoke tests inside a privileged Linux amd64 container.
# Safe to run from macOS hosts; requires Docker.
set -euo pipefail

export PATH="/bin:/usr/bin:/opt/homebrew/bin:${HOME}/.cargo/bin:${PATH}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

if ! command -v docker >/dev/null 2>&1; then
  echo "docker not found — install Docker to run USER ns live smokes from macOS" >&2
  exit 1
fi

docker run --rm --privileged --platform linux/amd64 \
  -v "${ROOT}:/work" -w /work \
  -e NAMESPACES_INTEGRATION=1 \
  -e EIDOLON_SANDBOX_NS_USER=1 \
  -e CARGO_TARGET_DIR=/work/.target-linux-user-ns-smoke \
  rust:1-bookworm \
  bash -lc '
    set -euo pipefail
    export PATH="/bin:/usr/bin:$PATH"
    apt-get update -qq
    apt-get install -y -qq uidmap >/dev/null
    rustup component add rustfmt clippy 2>/dev/null || true
    cargo test -p eidolon-sandbox --features sandbox-namespaces --locked \
      --test enforcement_linux user_ns -- --ignored --nocapture
  '

echo "USER ns live smokes finished inside Docker (linux/amd64, privileged)."
