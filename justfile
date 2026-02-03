# Justfile for Hayate development

# List available commands
default:
    @just --list

# Run Hayate indexer
run *ARGS:
    cargo run -- {{ARGS}}

# Run on preprod from genesis
run-preprod:
    cargo run -- --network preprod --from-genesis

# Run tests
test:
    cargo test

# Run tests with output
test-verbose:
    cargo test -- --nocapture

# Run specific test suite
test-suite suite:
    cargo test --test {{suite}}

# Run tests with coverage
test-coverage:
    cargo tarpaulin --out Html --output-dir target/coverage
    @echo "Coverage: target/coverage/index.html"

# Run only unit tests
test-unit:
    cargo test --lib

# Run only integration tests
test-integration:
    cargo test --test integration_tests

# Run rewards tests
test-rewards:
    cargo test --test rewards_tracker_tests

# Run block processor tests
test-blocks:
    cargo test --test block_processor_tests

# Build
build:
    cargo build

# Build release
build-release:
    cargo build --release

# Run clippy
lint:
    cargo clippy -- -D warnings

# Format code
fmt:
    cargo fmt
    nix fmt

# Clean
clean:
    cargo clean
    rm -rf target/

# Clean everything including cargo caches
clean-all:
    cargo clean
    rm -rf target/
    rm -rf ~/.cargo/registry/cache/ || true
    rm -rf ~/.cargo/git/checkouts/ || true

# Check everything
check: lint test

# Watch and test
watch:
    cargo watch -x test

# Build with Nix
nix-build:
    nix build

# Check Nix flake
nix-check:
    nix flake check

# Development cycle
dev: fmt check
