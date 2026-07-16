# Contributing

This is a Rust-based Zed extension compiled to `wasm32-wasip2`. Keep changes focused on resolving
and launching `dprint lsp`; formatting behavior belongs in dprint itself.

## Setup

Install Rust with `rustup`. The repository's `rust-toolchain.toml` supplies stable Rust, rustfmt,
Clippy, and the WASM target.

Install the optional repository tools with:

```sh
just install-tools
```

## Checks

Run the full local validation before opening a pull request:

```sh
just lint
just build-wasm
```

`just lint` checks formatting, runs Clippy with warnings denied, runs unit tests, validates the WASM
target, and checks the non-Rust files with dprint.

## Runtime testing

Install the repository using `zed: install dev extension`, then test the affected resolution paths:

1. An explicit `lsp.dprint.binary.path`.
2. A project-local npm dependency with `node_modules/.bin/dprint` installed.
3. A `dprint` binary on `PATH`.
4. Auto-install with no configured, local, or system binary.

Also verify custom `binary.arguments` and `binary.env` when changing command construction. For
auto-install changes, test macOS, Linux, and Windows behavior where practical and confirm current
asset names against the official `dprint/dprint` releases.

Use `zed: open log` or launch Zed with `zed --foreground` when diagnosing startup failures. Include
the Zed version, OS and architecture, resolved binary source, dprint version, command arguments, and
relevant logs in bug reports.

## Release checklist

1. Update `CHANGELOG.md`.
2. Keep the version identical in `Cargo.toml`, `Cargo.lock`, and `extension.toml`.
3. Run `just lint`, `just build-wasm`, and `just audit`.
4. Test the dev extension in Zed.
5. Coordinate the registry submodule update with `zed-industries/extensions`.

Do not commit `extension.wasm`, credentials, or files unrelated to the extension's runtime needs.
