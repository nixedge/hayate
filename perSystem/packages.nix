{inputs, ...}: {
  perSystem = {
    inputs',
    system,
    config,
    lib,
    pkgs,
    ...
  }: let
    # Use nightly toolchain - required by amaru dependencies
    toolchain = with inputs'.fenix.packages;
      combine [
        minimal.rustc
        minimal.cargo
        complete.clippy
        complete.rustfmt
      ];

    craneLib = (inputs.crane.mkLib pkgs).overrideToolchain toolchain;

    src = lib.fileset.toSource {
      root = ./..;
      fileset = lib.fileset.unions [
        ../Cargo.lock
        ../Cargo.toml
        ../build.rs
        ../src
        ../proto
        ../benches
      ];
    };

    # Extract pname and version from Cargo.toml
    crateInfo = craneLib.crateNameFromCargoToml {cargoToml = ../Cargo.toml;};

    commonArgs = {
      inherit src;
      inherit (crateInfo) pname version;
      strictDeps = true;

      nativeBuildInputs = with pkgs; [
        pkg-config
        protobuf
        installShellFiles
      ];

      # Link cardano-lsm from flake input as a path dependency
      preConfigure = ''
        mkdir -p ../cardano-lsm-rust
        cp -r ${inputs.cardano-lsm}/* ../cardano-lsm-rust/
        chmod -R +w ../cardano-lsm-rust
      '';

      meta = {
        description = "Hayate (疾風) - Swift Cardano full node indexer";
        license = lib.licenses.asl20;
        mainProgram = "hayate";
      };
    };

    # Build dependencies separately for caching
    cargoArtifacts = craneLib.buildDepsOnly commonArgs;
  in {
    packages = {
      default = config.packages.hayate;

      # Hayate indexer
      hayate = craneLib.buildPackage (commonArgs
        // {
          inherit cargoArtifacts;
          doCheck = true;

          # Generate and install shell completions
          postInstall = ''
            # Generate completions for each shell
            for shell in bash zsh fish; do
              $out/bin/hayate completions $shell > hayate.$shell
            done

            # Install completions
            installShellCompletion --bash hayate.bash
            installShellCompletion --zsh hayate.zsh
            installShellCompletion --fish hayate.fish
          '';
        });
    };
  };
}
