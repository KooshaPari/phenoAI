#!/bin/sh
# fake-nvms.sh — emulates `nvms run --config <path>` for hermetic tests.
MARKER="${FAKE_MARKER:-FAKE-NVMS-ALIVE rev-1}"
echo "$MARKER"
if [ "${HERMETIC_QUIET:-0}" = "1" ]; then
  exit 0
fi
SLEEP_SECS="${HERMETIC_SLEEP_SECS:-30}"
sleep "$SLEEP_SECS"
echo "FAKE-NVMS-DONE"
exit 0
