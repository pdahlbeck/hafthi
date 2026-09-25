#!/bin/sh
set -eu

test_dir=$(mktemp -d)
trap 'rm -r "$test_dir"' EXIT
export HAFTHI_GHOST_DIR="$test_dir/state"
# GitHub stores scripts/g without its executable bit; install.sh sets it on install.
cp scripts/g "$test_dir/g"
chmod +x "$test_dir/g"
g_path="$test_dir/g"

# Package managers and password prompts must never start a Ghost Task.
if "$g_path" yay > "$test_dir/rejected" 2>&1; then
    echo 'g yay unexpectedly started' >&2
    exit 1
fi
grep -q 'interactive terminal' "$test_dir/rejected"
[ ! -d "$HAFTHI_GHOST_DIR/ghost1" ]

# Run inside a real pseudo terminal: a nested program must not reopen /dev/tty
# and print over the interactive prompt.
script -q -c "HAFTHI_GHOST_DIR='$HAFTHI_GHOST_DIR' '$g_path' /bin/sh -c 'printf GHOST_TTY_LEAK >/dev/tty'; sleep 1" /dev/null </dev/null > "$test_dir/visible"
if grep -q GHOST_TTY_LEAK "$test_dir/visible"; then
    echo 'Ghost Task wrote directly to the terminal' >&2
    exit 1
fi
[ -f "$HAFTHI_GHOST_DIR/ghost1/exit" ]
[ "$(cat "$HAFTHI_GHOST_DIR/ghost1/exit")" -ne 0 ]
grep -q '/dev/tty' "$HAFTHI_GHOST_DIR/ghost1/output"
