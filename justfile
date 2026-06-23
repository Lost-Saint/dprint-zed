alias f := fmt
alias l := lint

# Install internal tools to manage the repository
install-tools:
  cargo install cargo-binstall
  cargo binstall cargo-audit dprint knope

# Format files
fmt:
 cargo fmt
 dprint fmt

# Build the WebAssembly artifact that Zed runs
build-wasm:
  rustup target add wasm32-wasip2
  cargo build --release --target wasm32-wasip2

# Lint files
lint:
  cargo clippy --all-targets -- -D warnings
  cargo check --target wasm32-wasip2
  cargo test
  dprint check

# Audit dependencies
audit:
  cargo audit
