{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, git-hooks, ... }:
    flake-utils.lib.eachDefaultSystem
      (system:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs { inherit system overlays; };

          # Define the package
          midi-actions = pkgs.rustPlatform.buildRustPackage {
            pname = "midi-actions";
            version = "0.1.0";
            src = ./.; # Assumes Cargo.toml is in the root
            cargoLock.lockFile = ./Cargo.lock;

            # Native dependencies needed for midir (ALSA on Linux)
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.alsa-lib ] ++ pkgs.lib.optionals pkgs.stdenv.isLinux [ pkgs.udev ];
          };
        in
        {
          packages.default = midi-actions;

          checks.pre-commit-checks = git-hooks.lib.${system}.run {
            src = ./.;
            hooks = {
              rustfmt.enable = true;
              nixpkgs-fmt.enable = true;
            };
          };

        # Development shell for 'nix develop'
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            cargo
            rustc
            rust-analyzer
            pkg-config
            alsa-lib
          ] ++ lib.optionals stdenv.isLinux [ udev ];

          shellHook = ''
            ${git-hooks.lib.${system}.run {
              src = ./.;
              hooks = {
                rustfmt.enable = true;
                nixpkgs-fmt.enable = true;
              };
            }}/bin/install
          '';
        };
        }
      ) // {
      # NixOS module for configuration
      nixosModules.default = { config, lib, ... }: {
        options.services.midi-actions = {
          enable = lib.mkEnableOption "Enable midi-actions service";
          user = lib.mkOption {
            type = lib.types.str;
            default = "yourusername";
            description = "User to add to uinput and input groups";
          };
        };

        config = lib.mkIf config.services.midi-actions.enable {
          # Enable uinput
          hardware.uinput.enable = true;

          # Add user to groups
          users.users.${config.services.midi-actions.user}.extraGroups = [ "uinput" "input" ];

          # Custom udev rule
          services.udev.extraRules = ''
            KERNEL=="uinput", MODE="0660", GROUP="uinput", OPTIONS+="static_node=uinput"
          '';
        };
      };
    };
}
