#!/bin/sh
# Every gate this project holds itself to, each reported separately.
#
# Chaining these with && behind a pipe hides failures: a `cargo clippy ... | tail`
# reports the exit status of `tail`. That happened once and a clippy failure was
# committed, so each gate is checked and reported on its own line here.

set -e
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."

gate() {
    printf '%-18s' "$1"
    shift
    if "$@" >/dev/null 2>&1; then
        echo ok
    else
        echo FAILED
        echo
        echo "--- $* ---"
        "$@" 2>&1 | tail -30
        exit 1
    fi
}

gate 'fmt'           cargo fmt --check
gate 'build'         cargo build --all-targets
gate 'build release' cargo build --release
gate 'build faults'  cargo build --all-targets --features fault-injection
gate 'clippy'        cargo clippy --all-targets -- -D warnings
gate 'clippy faults' cargo clippy --all-targets --features fault-injection -- -D warnings
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
