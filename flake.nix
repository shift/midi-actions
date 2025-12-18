{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        
        # Define the package
        midi-daemon = pkgs.rustPlatform.buildRustPackage {
          pname = "midi-daemon";
          version = "0.1.0";
          src = ./.; # Assumes Cargo.toml is in the root
          cargoLock.lockFile = ./Cargo.lock;
          
          # Native dependencies needed for midir (ALSA)
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.alsa-lib pkgs.udev ];
        };
      in
      {
        packages.default = midi-daemon;
        
        # Development shell for 'nix develop'
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            cargo rustc rust-analyzer pkg-config alsa-lib udev
          ];
        };
      }
    );
}
