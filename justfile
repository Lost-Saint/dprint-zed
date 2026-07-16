alias f := fmt
alias l := lint

# Install internal tools to manage the repository
install-tools:
  cargo install cargo-binstall
  cargo binstall cargo-audit dprint knope

# Format files
fmt:
  cargo fmt --all
  dprint fmt

# Build the WebAssembly artifact that Zed runs
build-wasm:
  cargo build --release --target wasm32-wasip2

# Lint files
lint:
  cargo fmt --all -- --check
  cargo clippy --all-targets --all-features -- -D warnings
  cargo test --all-features
  cargo check --target wasm32-wasip2
  dprint check

# Audit dependencies
audit:
  cargo audit
