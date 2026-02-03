{inputs, ...}: {
  perSystem = {
    system,
    config,
    lib,
    pkgs,
    ...
  }: {
    packages = {
      default = config.packages.hayate;
      
      # Hayate indexer
      hayate = let
        naersk-lib = inputs.naersk.lib.${system};
      in
        naersk-lib.buildPackage {
          pname = "hayate";
          version = "0.1.0";

          src = with lib.fileset;
            toSource {
              root = ./..;
              fileset = unions [
                ../Cargo.lock
                ../Cargo.toml
                ../src
              ];
            };

          nativeBuildInputs = with pkgs; [
            pkg-config
            protobuf
          ];
          
          # cardano-lsm will be available via path dependency
          
          doCheck = true;

          meta = {
            description = "Hayate (疾風) - Swift Cardano full node indexer";
            license = lib.licenses.asl20;
            mainProgram = "hayate";
          };
        };
    };
  };
}
