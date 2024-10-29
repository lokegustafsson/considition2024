{
  inputs = {
    systems.url = "github:nix-systems/default";
    flake-utils = {
      url = "github:numtide/flake-utils";
      inputs.systems.follows = "systems";
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.flake-utils.follows = "flake-utils";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs:
    inputs.flake-utils.lib.eachSystem
    [ inputs.flake-utils.lib.system.x86_64-linux ] (system:
      let
        pkgs = import inputs.nixpkgs {
          inherit system;
          overlays =
            [ inputs.rust-overlay.overlays.default ];
        };

        lib = pkgs.lib;

        cargoNix = import ./Cargo.nix { inherit pkgs; };

        considition2024 = cargoNix.workspaceMembers.considition2024.build;
      in {
        formatter = pkgs.writeShellApplication {
          name = "format";
          runtimeInputs =
            [ pkgs.rust-bin.stable.latest.default pkgs.nixfmt-classic ];
          text = ''
            set -v
            cargo fmt
            find . -name '*.nix' | grep -v Cargo.nix | xargs nixfmt'';
        };

        devShell = pkgs.mkShell {
          nativeBuildInputs = let p = pkgs;
          in [
            p.bashInteractive
            p.crate2nix
            (p.rust-bin.stable.latest.default.override {
              extensions = [ "rust-src" "rust-analyzer" ];
            })
          ];

          shellHook = ''
            git rev-parse --is-inside-work-tree > /dev/null && [ -n "$CARGO_TARGET_DIR_PREFIX" ] && \
            export CARGO_TARGET_DIR="$CARGO_TARGET_DIR_PREFIX$(git rev-parse --show-toplevel)"
          '';
        };

        checks.default = considition2024.override { runTests = true; };

        packages = rec {
          default = considition2024;
          inherit considition2024;
        };
      });
}
