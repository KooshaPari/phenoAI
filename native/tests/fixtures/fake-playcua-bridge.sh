#!/bin/sh
# fake-playcua-bridge.sh — shell wrapper that execs the Python fake bridge.
# Prefer this as PLAYCUA_BRIDGE_BIN on Unix hermetic tests.
DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
exec python3 "$DIR/fake-playcua-bridge.py"
