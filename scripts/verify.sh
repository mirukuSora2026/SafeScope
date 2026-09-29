#!/bin/sh
# Every gate this project holds itself to, each reported separately.
#
# Chaining these with && behind a pipe hides failures: a `cargo clippy ... | tail`
# reports the exit status of `tail`. That happened once and a clippy failure was
# committed, so each gate is checked and reported on its own line here.

set -e
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."

# Keeps the whole output of a failing gate. Tailing it once hid which test
# suite had failed while every suite printed "ok", which cost an investigation.
log=$(mktemp -d)/verify.log

gate() {
    printf '%-18s' "$1"
    shift
    if "$@" >"$log" 2>&1; then
        echo ok
    else
        echo FAILED
        echo
        echo "--- $* ---"
        grep -E 'FAILED|panicked at|^error|failures:' -A 6 "$log" | head -60
        echo
        echo "full output: $log"
        exit 1
    fi
}

gate 'fmt'           cargo fmt --check
gate 'build'         cargo build --all-targets
gate 'build release' cargo build --release
gate 'build faults'  cargo build --all-targets --features fault-injection
gate 'clippy'        cargo clippy --all-targets -- -D warnings
gate 'clippy faults' cargo clippy --all-targets --features fault-injection -- -D warnings
# Before the tests, because one of them drives the binary this produces. The
# plugin's manifest names `bin/safescope.exe`, which is not checked in, so without
# this a fresh clone has a plugin that silently does nothing.
gate 'package'       ./scripts/build-plugin.sh
gate 'tests'         cargo test
gate 'tests faults'  cargo test --features fault-injection

printf '%-18s' 'file lengths'
over=$(find src tests -name '*.rs' -exec wc -l {} + | awk '$1 > 500 && $2 != "total" {print $2}')
if [ -z "$over" ]; then
    echo ok
else
    echo "FAILED: $over"
    exit 1
fi

printf '%-18s' 'test count'
cargo test 2>&1 | grep -cE '^test .* \.\.\. ok'
