#!/usr/bin/env bash
# Run georgikon (or georgikon-launcher) on NixOS with the correct runtime library paths.
# Usage:
#   ./run.sh                      # run the game (combined client+server)
#   ./run.sh --server             # headless server
#   ./run.sh --client             # client only
#   ./run.sh launcher             # run the launcher instead
set -eu

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# ── pkg-config setup (mirrors gcheck.sh) ────────────────────────────────────
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
export PATH="/nix/store/1m05k7xgfnw6jc21xxk5681ni3ar97wf-pkg-config-wrapper-0.29.2/bin:$PATH"

# ── runtime library paths ────────────────────────────────────────────────────
libdir() { pkg-config --variable=libdir "$1" 2>/dev/null; }

LIBDIRS=(
  "$(libdir xkbcommon)"
  "$(libdir wayland-client)"
  "$(libdir alsa)"
  "$(libdir libudev)"
  "$(libdir x11)"
  "$(libdir xcb)"
  "$(libdir vulkan)"        # libvulkan.so.1 — wgpu dlopen's this to reach the ICD
  "$(libdir egl)"          # libEGL.so.1 — glvnd dispatcher, needed by glutin/eframe
  "$(libdir gl)"           # libGL.so.1  — glvnd dispatcher
  /run/opengl-driver/lib   # GPU driver ICDs + GL (stable NixOS symlink)
)
IFS=:; export LD_LIBRARY_PATH="${LIBDIRS[*]}${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"; unset IFS

# ── Vulkan / EGL ICD discovery ───────────────────────────────────────────────
# /run/opengl-driver is the stable NixOS symlink updated by nixos-rebuild switch.
# The Vulkan loader searches XDG_DATA_DIRS for share/vulkan/icd.d/*.json, which
# covers both the NVIDIA ICD and all Mesa ICDs present there.
export XDG_DATA_DIRS="/run/opengl-driver/share${XDG_DATA_DIRS:+:$XDG_DATA_DIRS}"
# EGL vendor libraries (needed if wgpu falls back from Vulkan to OpenGL/EGL)
export __EGL_VENDOR_LIBRARY_DIRS="/run/opengl-driver/share/glvnd/egl_vendor.d${__EGL_VENDOR_LIBRARY_DIRS:+:$__EGL_VENDOR_LIBRARY_DIRS}"

# Bevy's FileAssetReader checks CARGO_MANIFEST_DIR to locate the assets/ folder.
# cargo run sets it automatically; direct binary execution does not.
export CARGO_MANIFEST_DIR="$SCRIPT_DIR"

# ── resolve binary ───────────────────────────────────────────────────────────
if [[ "${1:-}" == "launcher" ]]; then
  shift
  BIN="$SCRIPT_DIR/launcher/target/release/georgikon-launcher"
  [[ -x "$BIN" ]] || { echo "Launcher not built. Run: cd launcher && cargo build --release"; exit 1; }
else
  BIN="$SCRIPT_DIR/target/release/georgikon"
  [[ -x "$BIN" ]] || { echo "Game not built. Run: cargo build --release"; exit 1; }
fi

exec "$BIN" "$@"
