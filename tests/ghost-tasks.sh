#!/bin/sh
set -eu

test_dir=$(mktemp -d)
trap 'rm -r "$test_dir"' EXIT
export HAFTHI_GHOST_DIR="$test_dir/state"
# GitHub stores scripts/g without its executable bit; install.sh sets it on install.
cp scripts/g "$test_dir/g"
chmod +x "$test_dir/g"
g_path="$test_dir/g"

# Interactive commands open a separate Hafþi window with a real PTY,
# and do not create a silent Ghost Task or touch the main terminal.
mkdir "$test_dir/bin"
cat > "$test_dir/bin/yay" <<'YAY'
#!/bin/sh
exit 0
YAY
cat > "$test_dir/fake-hafthi" <<'APP'
#!/bin/sh
printf '%s\n' "$@" > "$HAFTHI_TEST_RECORD"
APP
chmod +x "$test_dir/bin/yay" "$test_dir/fake-hafthi"
HAFTHI_BIN="$test_dir/fake-hafthi" HAFTHI_TEST_RECORD="$test_dir/interactive-args" \
    PATH="$test_dir/bin:$PATH" "$g_path" yay -Syu > "$test_dir/interactive-message"
attempt=0
while [ ! -f "$test_dir/interactive-args" ] && [ "$attempt" -lt 30 ]; do
    sleep 0.1
    attempt=$((attempt + 1))
done
[ "$(cat "$test_dir/interactive-args")" = "$(printf '%s\n' --interactive yay -Syu)" ]
grep -q 'separate Hafþi window' "$test_dir/interactive-message"
[ ! -d "$HAFTHI_GHOST_DIR/ghost1" ]

# A Ghost Tasks inbox routes interactive arguments to the parent window's PTY.
# The test process acknowledges the request without starting a real package manager.
mkdir "$test_dir/inbox"
(
    attempt=0
    while [ ! -f "$test_dir/inbox/ghost1" ] && [ "$attempt" -lt 30 ]; do
        sleep 0.1
        attempt=$((attempt + 1))
    done
    [ -f "$test_dir/inbox/ghost1" ]
    printf '%s\n' "$$" > "$HAFTHI_GHOST_DIR/ghost1/pid"
) &
inbox_reader=$!
HAFTHI_GHOST_INBOX="$test_dir/inbox" PATH="$test_dir/bin:$PATH" "$g_path" yay -Syu > "$test_dir/drawer-message"
wait "$inbox_reader"
grep -q 'Ctrl+G opens its drawer' "$test_dir/drawer-message"
[ "$(tr '\000' '\n' < "$test_dir/inbox/ghost1")" = "$(printf '%s\n' "$PWD" yay -Syu)" ]
printf '0\n' > "$HAFTHI_GHOST_DIR/ghost1/exit"
rm -r "$HAFTHI_GHOST_DIR/ghost1"

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
