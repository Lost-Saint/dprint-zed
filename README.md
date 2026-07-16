# dprint for Zed

This extension runs [`dprint lsp`](https://dprint.dev/install/) so Zed can use dprint as a
language-server formatter. Formatting behavior comes from the dprint configuration and plugins in
your project; the extension does not implement a formatter itself.

## Binary resolution

The extension resolves `dprint` in this order:

1. `lsp.dprint.binary.path` in Zed settings.
2. `node_modules/.bin/dprint` when `package.json` declares a `dprint` dependency.
3. A `dprint` executable on the worktree's `PATH`.
4. The latest stable binary from the official `dprint/dprint` GitHub release.

Auto-install supports macOS, Linux, and Windows on x86-64 or AArch64. The extension caches the
resolved auto-installed binary for its lifetime and removes older downloads after a successful
installation.

## Configuration

The default command is:

```sh
dprint lsp
```

You can override its path, arguments, and environment in Zed's `settings.json`:

```json
{
  "lsp": {
    "dprint": {
      "binary": {
        "path": "/absolute/path/to/dprint",
        "arguments": ["lsp"],
        "env": {
          "DPRINT_CACHE_DIR": "/absolute/path/to/cache"
        }
      }
    }
  }
}
```

The extension also forwards Zed's `initialization_options` and `settings` values to the language
server. dprint normally discovers `dprint.json`, `dprint.jsonc`, `.dprint.json`, or `.dprint.jsonc`
from the file being formatted. Prefer dprint's standard configuration discovery over editor-only
settings.

## Languages

The extension registers dprint for the languages listed in [extension.toml](extension.toml). Actual
formatting support depends on the plugins in your dprint configuration.

## Development

The checked-in Rust toolchain configuration installs the components and `wasm32-wasip2` target
needed by Zed.

```sh
just lint
just build-wasm
```

Equivalent commands:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo check --target wasm32-wasip2
cargo build --release --target wasm32-wasip2
```

The release build is written to:

```text
target/wasm32-wasip2/release/zed_dprint.wasm
```

Zed builds and packages this artifact when installing the extension. Do not commit a generated
`extension.wasm`.

### Install as a dev extension

1. Open Zed's Extensions page.
2. Run `zed: install dev extension`.
3. Select this repository.
4. Use **Rebuild** on the Extensions page after making changes.

For runtime failures, inspect `Zed.log` with `zed: open log`, or launch Zed with `zed --foreground`
for verbose output.

## Publishing

Zed publishes extensions by referencing their repository from
[`zed-industries/extensions`](https://github.com/zed-industries/extensions). Keep the versions in
`Cargo.toml`, `Cargo.lock`, and `extension.toml` aligned, update `CHANGELOG.md`, and submit a
registry PR that advances the extension submodule.

The `dprint` ID already belongs to the existing marketplace extension. Replacing its repository
requires coordination with Zed maintainers; that handoff is tracked in
[`zed-industries/extensions#5756`](https://github.com/zed-industries/extensions/issues/5756).

See Zed's [extension development guide](https://zed.dev/docs/extensions/developing-extensions) for
the current publishing requirements.

## License

[MIT](LICENSE)
