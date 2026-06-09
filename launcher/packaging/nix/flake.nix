{
  description = "Georgikon Launcher — social MMORPG client launcher";

  inputs = {
    nixpkgs.url     = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay    = {
      url    = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = pkgs.rust-bin.stable.latest.default;

        launcher = pkgs.callPackage ./default.nix { };
      in
      {
        packages = {
          georgikon-launcher = launcher;
          default             = launcher;
        };

        devShells.default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            rustToolchain
            pkg-config
          ];
          buildInputs = with pkgs; [
            libxkbcommon
            wayland
            libGL
            xorg.libX11
            xorg.libXcursor
            xorg.libXrandr
            xorg.libXi
          ];
          # Make sure eframe can find Wayland/GL at link time in the dev shell
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
            pkgs.wayland
            pkgs.libGL
            pkgs.libxkbcommon
          ];
          CARGO_MANIFEST_DIR = toString ./../..;
        };

        # NixOS module so users can install via their system config
        nixosModules.default = { config, lib, pkgs, ... }: {
          options.programs.georgikon-launcher.enable =
            lib.mkEnableOption "Georgikon Launcher";
          config = lib.mkIf config.programs.georgikon-launcher.enable {
            environment.systemPackages = [ launcher ];
          };
        };
      }
    );
}
