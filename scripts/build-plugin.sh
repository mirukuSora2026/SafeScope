#!/bin/sh
# Builds the engine and puts it where the plugin manifests expect it.
#
# The binary is not checked in: it is platform-specific and would go stale
# against the source beside it.

set -e
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."

cargo build --release
mkdir -p plugin/bin

# Removed before copying, not overwritten. Writing over a Mach-O the kernel has
# already seen invalidates its signature, and macOS then kills the process with
# SIGKILL and no message — which looks exactly like a server that never started.
rm -f plugin/bin/safescope
cp target/release/safescope plugin/bin/safescope

echo "plugin/bin/safescope  <-  $(cargo pkgid | sed 's/.*#//')"
