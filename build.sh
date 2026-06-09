#!/usr/bin/env bash
# Build georgikon on NixOS with the correct pkg-config and linker environment.
# Usage:
#   ./build.sh                  # debug build (game only)
#   ./build.sh --release        # release build (game + launcher)
#   ./build.sh --release --server  # (any extra cargo args are forwarded to the game build)
set -eu

export PATH="/nix/store/1m05k7xgfnw6jc21xxk5681ni3ar97wf-pkg-config-wrapper-0.29.2/bin:$PATH"

PCDIRS=(
  /nix/store/v3jm5z02mx668hx7gwd9kwxqxpfyd62i-wayland-1.25.0-dev/lib/pkgconfig
  /nix/store/rlzm88ay9s27biynqbypdsjb64lypacy-libxkbcommon-1.13.1-dev/lib/pkgconfig
  /nix/store/aj7zqrfxvg96ldyph7fly6qgprcm2krv-libffi-3.5.2-dev/lib/pkgconfig
  /nix/store/6fbkwxv11i13lsgq8w1lzlaxm4c2a90b-libxcb-1.17.0-dev/lib/pkgconfig
  /nix/store/1rhchilgcirwrwmq4h8xqldkn8lx209x-libx11-1.8.13-dev/lib/pkgconfig
  /nix/store/p8hlxg73dkxaznhql8pdkgrvf0xsy68y-alsa-lib-1.2.15.3-dev/lib/pkgconfig
  /nix/store/n0a93nl0ydkgnwwyqrrib8nrd9g51gi7-systemd-minimal-libs-260.1-dev/lib/pkgconfig
  /nix/store/n0a93nl0ydkgnwwyqrrib8nrd9g51gi7-systemd-minimal-libs-260.1-dev/share/pkgconfig
  /nix/store/hw5vrqqsjwq975zkysgr9p7whxzfkhdq-vulkan-loader-1.4.341.0-dev/lib/pkgconfig
  /nix/store/0jngqd8asdfjk44si8yalhrrzvyk6azh-libglvnd-1.7.0-dev/lib/pkgconfig
)
IFS=:; export PKG_CONFIG_PATH="${PCDIRS[*]}"; unset IFS

export RUSTFLAGS="-C link-arg=-fuse-ld=mold"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "==> Building game…"
cargo build "$@"

# The launcher is a separate workspace and is only ever shipped as a release binary.
# Build it whenever --release is among the forwarded args.
if [[ " $* " == *" --release "* ]] || [[ "$*" == "--release" ]]; then
  echo "==> Building launcher (release)…"
  (cd "$SCRIPT_DIR/launcher" && cargo build --release)
  echo "    launcher/target/release/georgikon-launcher"
fi
