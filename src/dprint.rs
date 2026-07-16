//! Zed extension entry point for running `dprint lsp`.
//!
//! The extension is intentionally small: it resolves a suitable `dprint` binary, supplies the
//! arguments Zed should use, and leaves formatting behavior to the dprint language server.

use std::{
  fs,
  path::{Path, PathBuf},
};
use zed_extension_api::{
  self as zed, Architecture, DownloadedFileType, GithubRelease, GithubReleaseOptions,
  LanguageServerId, LanguageServerInstallationStatus, Os, Worktree,
  serde_json::{self, Value},
  settings::LspSettings,
};

struct AutoInstallerConfig {
  github_repo: &'static str,
  release_folder_prefix: &'static str,
}

impl AutoInstallerConfig {
  fn binary_path(&self, version: &str, os: Os) -> PathBuf {
    let file_extension = match os {
      Os::Windows => ".exe",
      Os::Mac | Os::Linux => "",
    };

    PathBuf::from(self.release_dir_name(version)).join(format!("dprint{file_extension}"))
  }

  fn asset_name(&self, architecture: Architecture, os: Os) -> zed::Result<String> {
    let architecture = match architecture {
      Architecture::X8664 => "x86_64",
      Architecture::Aarch64 => "aarch64",
      Architecture::X86 => {
        return Err(
          concat!(
            "unsupported architecture: x86; ",
            "install dprint on your machine or worktree instead"
          )
          .into(),
        );
      }
    };

    let os = match os {
      Os::Linux => "unknown-linux-gnu",
      Os::Mac => "apple-darwin",
      Os::Windows => "pc-windows-msvc",
    };

    Ok(format!("dprint-{architecture}-{os}.zip"))
  }

  fn release_dir_name(&self, version: &str) -> String {
    format!("{}{version}", self.release_folder_prefix)
  }
}

struct WorktreeConfig {
  binary_basename: &'static str,
  worktree_binary_path: &'static str,
  windows_worktree_binary_path: &'static str,
  node_package_name: &'static str,
}

impl WorktreeConfig {
  fn binary_path(&self, os: Os) -> &str {
    match os {
      Os::Windows => self.windows_worktree_binary_path,
      Os::Mac | Os::Linux => self.worktree_binary_path,
    }
  }
}

struct InstallerConfig {
  auto_installer: AutoInstallerConfig,
  worktree: WorktreeConfig,
}

static DPRINT_CONFIG: InstallerConfig = InstallerConfig {
  auto_installer: AutoInstallerConfig {
    github_repo: "dprint/dprint",
    release_folder_prefix: "dprint-",
  },
  worktree: WorktreeConfig {
    binary_basename: "dprint",
    worktree_binary_path: "node_modules/.bin/dprint",
    windows_worktree_binary_path: "node_modules/.bin/dprint.cmd",
    node_package_name: "dprint",
  },
};

struct AutoInstaller {
  config: &'static AutoInstallerConfig,
  latest_release: GithubRelease,
  os: Os,
  architecture: Architecture,
  binary_path: PathBuf,
  release_dir_name: String,
}

impl AutoInstaller {
  fn try_new(config: &'static AutoInstallerConfig) -> zed::Result<Self> {
    let latest_release = zed::latest_github_release(
      config.github_repo,
      GithubReleaseOptions {
        require_assets: true,
        pre_release: false,
      },
    )?;

    let (os, architecture) = zed::current_platform();

    let binary_path = config.binary_path(&latest_release.version, os);
    let release_dir_name = config.release_dir_name(&latest_release.version);

    Ok(Self {
      config,
      latest_release,
      os,
      architecture,
      binary_path,
      release_dir_name,
    })
  }

  fn is_latest_release_installed(&self) -> bool {
    self.binary_path.is_file()
  }

  fn ensure_installed(&self, language_server_id: &LanguageServerId) -> zed::Result<&Path> {
    zed::set_language_server_installation_status(
      language_server_id,
      &LanguageServerInstallationStatus::CheckingForUpdate,
    );

    if self.is_latest_release_installed() {
      return Ok(&self.binary_path);
    }

    self.remove_old_releases()?;
    self.download_new_release(language_server_id)?;

    Ok(&self.binary_path)
  }

  fn remove_old_releases(&self) -> zed::Result<()> {
    for entry in
      fs::read_dir(".").map_err(|error| format!("failed to list working directory: {error}"))?
    {
      let entry = entry.map_err(|error| format!("failed to load directory entry: {error}"))?;
      let entry_path = entry.path();
      let Some(entry_name) = entry_path
        .file_name()
        .and_then(|file_name| file_name.to_str())
      else {
        continue;
      };

      if entry_name == self.release_dir_name
        || !entry_name.starts_with(self.config.release_folder_prefix)
      {
        continue;
      }

      let entry_metadata = entry
        .metadata()
        .map_err(|error| format!("failed to stat {entry_path:?}: {error}"))?;

      if entry_metadata.is_dir() {
        fs::remove_dir_all(&entry_path)
          .map_err(|error| format!("failed to remove directory {entry_path:?}: {error}"))?;
      } else {
        fs::remove_file(&entry_path)
          .map_err(|error| format!("failed to remove file {entry_path:?}: {error}"))?;
      }
    }

    Ok(())
  }

  fn download_new_release(&self, language_server_id: &LanguageServerId) -> zed::Result<()> {
    zed::set_language_server_installation_status(
      language_server_id,
      &LanguageServerInstallationStatus::Downloading,
    );

    let asset_name = self.config.asset_name(self.architecture, self.os)?;

    let asset = self
      .latest_release
      .assets
      .iter()
      .find(|asset| asset.name == asset_name)
      .ok_or_else(|| format!("no compatible asset found for {asset_name:?}"))?;

    zed::download_file(
      &asset.download_url,
      &self.release_dir_name,
      DownloadedFileType::Zip,
    )
  }
}

struct DprintExtension;

impl DprintExtension {
  fn resolve_language_server_binary(
    &self,
    language_server_id: &LanguageServerId,
    worktree: &Worktree,
    configured_path: Option<String>,
  ) -> zed::Result<String> {
    if let Some(path) = configured_path {
      return Ok(path);
    }

    let (os, _) = zed::current_platform();

    if self.worktree_declares_dprint_dependency(worktree) {
      let binary_path =
        Path::new(&worktree.root_path()).join(DPRINT_CONFIG.worktree.binary_path(os));
      return path_to_command(&binary_path);
    }

    if let Some(path) = worktree.which(DPRINT_CONFIG.worktree.binary_basename) {
      return Ok(path);
    }

    let binary_manager = AutoInstaller::try_new(&DPRINT_CONFIG.auto_installer)?;

    path_to_command(binary_manager.ensure_installed(language_server_id)?)
  }

  fn read_json_file(worktree: &Worktree, path: &str) -> zed::Result<Value> {
    let contents = worktree.read_text_file(path)?;
    serde_json::from_str(&contents).map_err(|error| format!("failed to parse {path}: {error}"))
  }

  fn worktree_declares_dprint_dependency(&self, worktree: &Worktree) -> bool {
    let node_package_name = DPRINT_CONFIG.worktree.node_package_name;

    Self::read_json_file(worktree, "package.json")
      .is_ok_and(|json| package_json_declares_dependency(&json, node_package_name))
      || Self::read_json_file(worktree, "deno.json")
        .is_ok_and(|json| deno_json_declares_import(&json, node_package_name))
  }
}

fn package_json_declares_dependency(package_json: &Value, package_name: &str) -> bool {
  json_object_has_non_null_key(package_json, "dependencies", package_name)
    || json_object_has_non_null_key(package_json, "devDependencies", package_name)
}

fn deno_json_declares_import(deno_json: &Value, package_name: &str) -> bool {
  json_object_has_non_null_key(deno_json, "imports", package_name)
}

fn json_object_has_non_null_key(json: &Value, object_key: &str, item_key: &str) -> bool {
  json
    .get(object_key)
    .and_then(Value::as_object)
    .and_then(|object| object.get(item_key))
    .is_some_and(|value| !value.is_null())
}

fn path_to_command(path: &Path) -> zed::Result<String> {
  path
    .to_str()
    .map(str::to_owned)
    .ok_or_else(|| format!("dprint executable path is not valid UTF-8: {path:?}"))
}

impl zed::Extension for DprintExtension {
  fn new() -> Self {
    Self
  }

  fn language_server_command(
    &mut self,
    language_server_id: &LanguageServerId,
    worktree: &Worktree,
  ) -> zed::Result<zed::Command> {
    let lsp_settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)?;
    let (configured_path, configured_arguments) = lsp_settings
      .binary
      .map_or((None, None), |binary| (binary.path, binary.arguments));

    let command =
      self.resolve_language_server_binary(language_server_id, worktree, configured_path)?;
    let args = configured_arguments.unwrap_or_else(|| vec!["lsp".into()]);

    Ok(zed::Command {
      command,
      args,
      env: Default::default(),
    })
  }
}

zed::register_extension!(DprintExtension);

#[cfg(test)]
mod tests {
  use super::*;
  use zed::serde_json::json;

  #[test]
  fn dprint_release_asset_names_match_supported_platforms() {
    assert_eq!(
      DPRINT_CONFIG
        .auto_installer
        .asset_name(Architecture::X8664, Os::Linux)
        .unwrap(),
      "dprint-x86_64-unknown-linux-gnu.zip"
    );
    assert_eq!(
      DPRINT_CONFIG
        .auto_installer
        .asset_name(Architecture::Aarch64, Os::Mac)
        .unwrap(),
      "dprint-aarch64-apple-darwin.zip"
    );
    assert_eq!(
      DPRINT_CONFIG
        .auto_installer
        .asset_name(Architecture::X8664, Os::Windows)
        .unwrap(),
      "dprint-x86_64-pc-windows-msvc.zip"
    );
  }

  #[test]
  fn x86_auto_install_is_unsupported() {
    let error = DPRINT_CONFIG
      .auto_installer
      .asset_name(Architecture::X86, Os::Linux)
      .unwrap_err();

    assert!(error.contains("unsupported architecture: x86"));
  }

  #[test]
  fn dprint_binary_paths_match_platform_conventions() {
    assert_eq!(
      DPRINT_CONFIG
        .auto_installer
        .binary_path("0.50.0", Os::Linux),
      PathBuf::from("dprint-0.50.0/dprint")
    );
    assert_eq!(
      DPRINT_CONFIG
        .auto_installer
        .binary_path("0.50.0", Os::Windows),
      PathBuf::from("dprint-0.50.0/dprint.exe")
    );
    assert_eq!(
      DPRINT_CONFIG.worktree.binary_path(Os::Windows),
      "node_modules/.bin/dprint.cmd"
    );
  }

  #[test]
  fn package_json_dependency_detection_checks_dependencies_and_dev_dependencies() {
    assert!(package_json_declares_dependency(
      &json!({ "dependencies": { "dprint": "1.0.0" } }),
      "dprint"
    ));
    assert!(package_json_declares_dependency(
      &json!({ "devDependencies": { "dprint": "1.0.0" } }),
      "dprint"
    ));
    assert!(!package_json_declares_dependency(
      &json!({ "dependencies": { "prettier": "1.0.0" } }),
      "dprint"
    ));
    assert!(!package_json_declares_dependency(
      &json!({ "dependencies": { "dprint": null } }),
      "dprint"
    ));
    assert!(!package_json_declares_dependency(
      &json!({ "dependencies": ["dprint"] }),
      "dprint"
    ));
  }

  #[test]
  fn deno_json_import_detection_checks_import_map() {
    assert!(deno_json_declares_import(
      &json!({ "imports": { "dprint": "npm:dprint" } }),
      "dprint"
    ));
    assert!(!deno_json_declares_import(
      &json!({ "imports": { "prettier": "npm:prettier" } }),
      "dprint"
    ));
    assert!(!deno_json_declares_import(
      &json!({ "imports": null }),
      "dprint"
    ));
  }

  #[test]
  fn command_paths_must_be_valid_utf8() {
    assert_eq!(
      path_to_command(Path::new("dprint-0.50.0/dprint")).unwrap(),
      "dprint-0.50.0/dprint"
    );
  }
}
