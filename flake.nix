{
  inputs = {
    nixpkgs = { url = "github:NixOS/nixpkgs/nixos-unstable"; };
    systems.url = "github:nix-systems/default";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, systems, rust-overlay, ... }@inputs:
    let
      eachSystem = f:
        nixpkgs.lib.genAttrs (import systems) (system:
          f (import nixpkgs {
            inherit system;
            config = { allowUnfree = true; };
            overlays = [
              rust-overlay.overlays.default
            ];
          }));
    in {

      devShells = eachSystem (pkgs: {
        default = pkgs.mkShell {
          hardeningDisable = [ "all" ];
          buildInputs = [ ];

          packages = [
            pkgs.gcc
            (pkgs.rust-bin.stable.latest.default.override {
              extensions = [ "rust-src" ];
              targets = [ "arm-unknown-linux-gnueabihf" ];
            })
            pkgs.cargo
            pkgs.rust-analyzer # language server
            pkgs.clippy # linter
            pkgs.rustfmt # formatter
            pkgs.gitAndTools.git-absorb
            pkgs.treefmt
          ];
        };
      });
    };
}