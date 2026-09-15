#!/bin/sh
# fake-sandbox.sh — emulates a sandbox wrapper for hermetic driver tests.
# When invoked as a wrapper: prints a marker, then execs remaining args
# (mirrors firejail `-- …` guest hand-off). With HERMETIC_QUIET=1, exits
# after the marker (spawn-only leg of hermetic_spawn_test).
MARKER="${FAKE_MARKER:-FAKE-SANDBOX-ALIVE rev-1}"
echo "$MARKER"
if [ "${HERMETIC_QUIET:-0}" = "1" ]; then
  exit 0
fi
if [ "$#" -gt 0 ]; then
  exec "$@"
fi
SLEEP_SECS="${HERMETIC_SLEEP_SECS:-30}"
sleep "$SLEEP_SECS"
echo "FAKE-SANDBOX-DONE"
exit 0
