# Changelog

All notable changes to this extension are documented in this file.

This project follows a pragmatic changelog format. Versions listed here refer to the Zed extension
package version in `extension.toml`.

## 0.1.1 - 2026-07-15

### Fixed

- Use the Windows npm command shim (`node_modules/.bin/dprint.cmd`) for workspace `dprint`
  dependencies on Windows.
- Make downloaded Unix binaries executable and verify the extracted binary before launching it.
- Report auto-install failures through Zed's language-server installation status.
- Forward configured binary environment variables, initialization options, and workspace settings.
- Avoid treating a Deno import as proof that a `node_modules` command shim exists.
- Update the transitive `anyhow` dependency to the release that fixes RUSTSEC-2026-0190.

### Changed

- Cache a valid auto-installed binary for the lifetime of the extension.
- Download a new release before removing older installations.
- Update the repository dprint Markdown plugin to the current listed version.
- Use the conventional `src/lib.rs` extension layout and add release WASM optimization settings.
- Correct the extension author and repository metadata.
- Add a pinned Rust development environment and continuous integration.

## 0.1.0

### Added

- Initial public release of the Zed extension.
- Registers a language server with id `dprint` for the languages listed in `extension.toml`.

### Implemented

- Language server command construction that runs `dprint` in LSP mode (defaults to `dprint lsp`).
- Binary resolution order:
  1. Uses `lsp.dprint.binary.path` from Zed settings if provided.
  2. Uses workspace npm binary path when the worktree declares `dprint` in `package.json`
     (`dependencies` or `devDependencies`) or `deno.json` (`imports`).
  3. Falls back to `dprint` found on `PATH`.
  4. Otherwise auto-downloads the latest stable `dprint` release from `dprint/dprint` GitHub
     releases and runs it.
- Auto-installer behavior:
  - Downloads the correct OS/architecture zip asset.
  - Removes previously downloaded `dprint-*` release folders/files before installing the new
    version.
  - Does not support 32-bit `x86` auto-install (manual binary configuration required on that
    architecture).
