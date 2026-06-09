# nixpkgs-compatible package for georgikon-launcher.
# Usage in nixpkgs: place in pkgs/games/georgikon-launcher/default.nix
# Usage standalone: nix-build -E 'with import <nixpkgs> {}; callPackage ./default.nix {}'
{ lib
, rustPlatform
, pkg-config
, libxkbcommon
, wayland
, libGL
, xorg
, stdenv
}:

rustPlatform.buildRustPackage {
  pname = "georgikon-launcher";
  version = "0.1.0";

  # When used inside the repo the src is two levels up from this file.
  # When submitted to nixpkgs, replace with a fetchFromGitHub call:
  #
  #   src = fetchFromGitHub {
  #     owner = "Remgr12";
  #     repo  = "georgikon";
  #     rev   = "v0.1.0";
  #     hash  = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
  #   };
  #   sourceRoot = "source/launcher";
  src = ../../..;
  sourceRoot = "source/launcher";

  cargoLock.lockFile = ../../Cargo.lock;

  nativeBuildInputs = [ pkg-config ];

  buildInputs =
    [ libxkbcommon libGL ]
    ++ lib.optionals stdenv.isLinux [
      wayland
      xorg.libX11
      xorg.libXcursor
      xorg.libXrandr
      xorg.libXi
    ];

  # eframe/winit looks up Wayland/GL at runtime via dlopen, so we patch the rpath.
  postInstall = lib.optionalString stdenv.isLinux ''
    patchelf --add-rpath ${lib.makeLibraryPath [ wayland libGL libxkbcommon ]} \
      $out/bin/georgikon-launcher

    install -Dm644 packaging/linux/georgikon-launcher.desktop \
      $out/share/applications/georgikon-launcher.desktop
  '';

  meta = {
    description = "Launcher for the Georgikon social MMORPG";
    homepage    = "https://github.com/Remgr12/georgikon";
    license     = lib.licenses.gpl3Plus;
    maintainers = with lib.maintainers; [ zsombor ];
    platforms   = lib.platforms.linux ++ lib.platforms.windows;
    mainProgram = "georgikon-launcher";
  };
}
