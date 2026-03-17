{
  description = "Hayate (疾風) - Swift Cardano full node indexer";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-parts.url = "github:hercules-ci/flake-parts";
    crane.url = "github:ipetkov/crane";
    fenix.url = "github:nix-community/fenix";
    fenix.inputs.nixpkgs.follows = "nixpkgs";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    treefmt-nix.url = "github:numtide/treefmt-nix";
    treefmt-nix.inputs.nixpkgs.follows = "nixpkgs";

    # Aiken smart contract compiler
    aiken.url = "github:aiken-lang/aiken/v1.1.21";
    # Don't follow nixpkgs - let Aiken use its own nixos-unstable + rust-overlay
  };

  outputs = {
    self,
    flake-parts,
    nixpkgs,
    ...
  } @ inputs: let
    inherit ((import ./flake/lib.nix {inherit inputs;}).flake.lib) recursiveImports;
  in
    flake-parts.lib.mkFlake {inherit inputs;} {
      imports =
        recursiveImports [
          ./flake
          ./perSystem
        ]
        ++ [
          inputs.treefmt-nix.flakeModule
        ];
      systems = [
        "x86_64-linux"
      ];
      perSystem = {system, ...}: {
        _module.args.pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [inputs.rust-overlay.overlays.default];
        };
      };
    }
    // {
      inherit inputs;
    };
}
