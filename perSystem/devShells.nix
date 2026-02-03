{
  perSystem = {
    config,
    pkgs,
    ...
  }: {
    devShells.default = with pkgs;
      mkShell {
        packages = [
          # Rust toolchain (nightly from rust-overlay)
          rust-bin.nightly.latest.default
          cmake
          pkg-config
          openssl
          zlib
          rust-analyzer
          
          # Protocol Buffers compiler (for UTxORPC)
          protobuf
          
          # Task runner
          just
          
          # Utilities
          jq
          fd
          ripgrep
          
          # Tree formatter
          config.treefmt.build.wrapper
        ];
        
        shellHook = ''
          echo "疾風 Hayate - Swift Cardano Indexer"
          echo ""
          echo "Rust: $(rustc --version)"
          echo "Cargo: $(cargo --version)"
          echo "Protoc: $(protoc --version)"
          echo ""
          echo "Commands:"
          echo "  just --list    # Show all commands"
          echo "  just run       # Run the indexer"
          echo "  just test      # Run tests"
        '';
      };
  };
}
