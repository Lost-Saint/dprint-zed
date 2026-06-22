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

# Lint files
lint:
  cargo clippy --all-targets -- -D warnings
  cargo test
  dprint check

# Audit dependencies
audit:
  cargo audit
