#!/usr/bin/env bash
# Build georgikon-launcher on NixOS / Linux.
# Sets RUSTFLAGS and PKG_CONFIG_PATH directly — does not depend on /tmp/gcheck.sh.
# Usage: ./build-linux.sh [extra cargo flags]   e.g.  ./build-linux.sh --release
set -eu

cd "$(dirname "$0")"

export RUSTFLAGS="-C link-arg=-fuse-ld=mold"

export PATH="/nix/store/1m05k7xgfnw6jc21xxk5681ni3ar97wf-pkg-config-wrapper-0.29.2/bin:$PATH"
export PKG_CONFIG_PATH="\
/nix/store/v3jm5z02mx668hx7gwd9kwxqxpfyd62i-wayland-1.25.0-dev/lib/pkgconfig:\
/nix/store/rlzm88ay9s27biynqbypdsjb64lypacy-libxkbcommon-1.13.1-dev/lib/pkgconfig:\
/nix/store/aj7zqrfxvg96ldyph7fly6qgprcm2krv-libffi-3.5.2-dev/lib/pkgconfig:\
/nix/store/6fbkwxv11i13lsgq8w1lzlaxm4c2a90b-libxcb-1.17.0-dev/lib/pkgconfig:\
/nix/store/1rhchilgcirwrwmq4h8xqldkn8lx209x-libx11-1.8.13-dev/lib/pkgconfig:\
/nix/store/p8hlxg73dkxaznhql8pdkgrvf0xsy68y-alsa-lib-1.2.15.3-dev/lib/pkgconfig:\
/nix/store/n0a93nl0ydkgnwwyqrrib8nrd9g51gi7-systemd-minimal-libs-260.1-dev/lib/pkgconfig:\
/nix/store/n0a93nl0ydkgnwwyqrrib8nrd9g51gi7-systemd-minimal-libs-260.1-dev/share/pkgconfig"

cargo build --release "$@"
echo "Binary: $(pwd)/target/release/georgikon-launcher"
