#!/bin/sh
# Builds the engine and puts it where the plugin manifests expect it.
#
# The binary is not checked in: it is platform-specific and would go stale
# against the source beside it.
#
# It is `bin/safescope.exe` on every platform. The host documents that on
# Windows a hook's command must resolve to "a real executable such as a .exe",
# and says nothing about whether an extensionless name finds one — so the name
# the manifests give is the one the documentation guarantees. Elsewhere the
# suffix is only a name. One set of manifests, checked by the same tests on
# every platform, is worth more than a tidier file name on two of them.

set -e
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."

cargo build --release
mkdir -p plugin/bin

# What cargo wrote: `safescope.exe` on Windows, `safescope` everywhere else.
built=target/release/safescope
if [ -f "$built.exe" ]; then
    built="$built.exe"
fi

# Removed before copying, not overwritten. Writing over a Mach-O the kernel has
# already seen invalidates its signature, and macOS then kills the process with
# SIGKILL and no message — which looks exactly like a server that never started.
# The old extensionless name goes too, so a stale build cannot sit beside it.
rm -f plugin/bin/safescope plugin/bin/safescope.exe
cp "$built" plugin/bin/safescope.exe

echo "plugin/bin/safescope.exe  <-  $(cargo pkgid | sed 's/.*#//')"
