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

struct AutoInstallerConfig<'config> {
  github_repo: &'config str,
  release_folder_prefix: &'config str,
}

trait AutoInstallerArtifacts {
  fn binary_path(&self, version: &str, os: Os) -> PathBuf;
  fn asset_name(&self, architecture: Architecture, os: Os) -> zed::Result<String>;
}

impl<'config> AutoInstallerArtifacts for AutoInstallerConfig<'config> {
  fn binary_path(&self, version: &str, os: Os) -> PathBuf {
    let file_extension = match os {
      Os::Windows => ".exe",
      Os::Mac | Os::Linux => "",
    };

    PathBuf::from(format!(
      "{folder_prefix}{version}/dprint{file_extension}",
      folder_prefix = self.release_folder_prefix,
      version = version,
      file_extension = file_extension,
    ))
  }

  fn asset_name(&self, architecture: Architecture, os: Os) -> zed::Result<String> {
    let architecture = match architecture {
      Architecture::X8664 => "x86_64",
      Architecture::Aarch64 => "aarch64",
      Architecture::X86 => {
        return Err(
          concat!(
            "Unsupported architecture: x86. ",
            "Consider manually installing dprint on your machine or worktree instead."
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

    Ok(format!("dprint-{architecture}-{os}.zip",))
  }
}

struct WorktreeConfig<'config> {
  binary_basename: &'config str,
  worktree_binary_path: &'config str,
  windows_worktree_binary_path: &'config str,
  node_package_name: &'config str,
}

impl WorktreeConfig<'_> {
  fn binary_path(&self, os: Os) -> &str {
    match os {
      Os::Windows => self.windows_worktree_binary_path,
      Os::Mac | Os::Linux => self.worktree_binary_path,
    }
  }
}

struct InstallerConfig<'config> {
  auto_installer: AutoInstallerConfig<'config>,
  worktree: WorktreeConfig<'config>,
}

const DPRINT_CONFIG: InstallerConfig = InstallerConfig {
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

struct AutoInstaller<'config> {
  config: &'config AutoInstallerConfig<'config>,
  latest_release: GithubRelease,
  os: Os,
  architecture: Architecture,
  language_server_id: LanguageServerId,
  binary_path: PathBuf,
}

impl<'config> AutoInstaller<'config> {
  fn try_new(
    config: &'config AutoInstallerConfig<'config>,
    language_server_id: &LanguageServerId,
  ) -> zed::Result<Self> {
    let latest_release = zed::latest_github_release(
      config.github_repo,
      GithubReleaseOptions {
        require_assets: true,
        pre_release: false,
      },
    )?;

    let (os, architecture) = zed::current_platform();

    let binary_path = config.binary_path(&latest_release.version, os);

    Ok(Self {
      config,
      latest_release,
      os,
      architecture,
      language_server_id: language_server_id.clone(),
      binary_path,
    })
  }

  fn is_latest_release_installed(&self) -> bool {
    self.binary_path.is_file()
  }

  fn ensure_installed(&self) -> zed::Result<PathBuf> {
    zed::set_language_server_installation_status(
      &self.language_server_id,
      &LanguageServerInstallationStatus::CheckingForUpdate,
    );

    if self.is_latest_release_installed() {
      return Ok(self.binary_path.clone());
    }

    self.remove_old_releases()?;
    self.download_new_release()?;

    Ok(self.binary_path.clone())
  }

  fn remove_old_releases(&self) -> zed::Result<()> {
    for entry in
      fs::read_dir(".").map_err(|error| format!("Failed to list working directory.{error:?}"))?
    {
      let entry = entry.map_err(|error| format!("Failed to load directory entry.{error:?}"))?;
      let entry_path = entry.path();
      let Some(entry_name) = entry_path
        .file_name()
        .and_then(|file_name| file_name.to_str())
      else {
        continue;
      };

      if !entry_name.starts_with(self.config.release_folder_prefix) {
        continue;
      }

      let entry_metadata = entry
        .metadata()
        .map_err(|error| format!("Failed to stat {entry_path:?}.{error:?}"))?;

      if entry_metadata.is_dir() {
        fs::remove_dir_all(&entry_path)
          .map_err(|error| format!("Failed to remove directory {entry_path:?}.{error:?}"))?;
      } else {
        fs::remove_file(&entry_path)
          .map_err(|error| format!("Failed to remove file {entry_path:?}.{error:?}"))?;
      }
    }

    Ok(())
  }

  fn download_new_release(&self) -> zed::Result<()> {
    zed::set_language_server_installation_status(
      &self.language_server_id,
      &LanguageServerInstallationStatus::Downloading,
    );

    let asset_name = self.config.asset_name(self.architecture, self.os)?;

    let asset = self
      .latest_release
      .assets
      .iter()
      .find(|asset| asset.name == asset_name)
      .ok_or_else(|| format!("No compatible asset found for {asset_name:?}."))?;

    zed::download_file(
      &asset.download_url,
      &format!(
        "{}{}",
        self.config.release_folder_prefix, self.latest_release.version
      ),
      DownloadedFileType::Zip,
    )
  }
}

struct DprintExtension;

impl DprintExtension {
  fn language_server_binary_path(
    &self,
    language_server_id: &LanguageServerId,
    worktree: &Worktree,
  ) -> zed::Result<String> {
    let lsp_settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)?;

    if let Some(path) = lsp_settings
      .binary
      .as_ref()
      .and_then(|binary| binary.path.clone())
    {
      return Ok(path);
    }

    let (os, _) = zed::current_platform();

    if self.worktree_declares_dprint_dependency(worktree) {
      return Ok(
        Path::new(&worktree.root_path())
          .join(DPRINT_CONFIG.worktree.binary_path(os))
          .to_string_lossy()
          .to_string(),
      );
    }

    if let Some(path) = worktree.which(DPRINT_CONFIG.worktree.binary_basename) {
      return Ok(path);
    }

    let binary_manager = AutoInstaller::try_new(&DPRINT_CONFIG.auto_installer, language_server_id)?;

    Ok(
      binary_manager
        .ensure_installed()?
        .to_string_lossy()
        .to_string(),
    )
  }

  fn language_server_arguments(
    &self,
    language_server_id: &LanguageServerId,
    worktree: &Worktree,
  ) -> zed::Result<Vec<String>> {
    let lsp_settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)?;

    if let Some(arguments) = lsp_settings
      .binary
      .as_ref()
      .and_then(|binary| binary.arguments.clone())
    {
      return Ok(arguments);
    }

    Ok(vec!["lsp".into()])
  }

  fn read_json_file(&self, worktree: &Worktree, path: &str) -> zed::Result<Value> {
    let contents = worktree.read_text_file(path)?;
    serde_json::from_str(&contents).map_err(|error| format!("failed to parse {path}: {error}"))
  }

  fn worktree_declares_dprint_dependency(&self, worktree: &Worktree) -> bool {
    let package_json = self.read_json_file(worktree, "package.json");
    let deno_json = self.read_json_file(worktree, "deno.json");
    let node_package_name = DPRINT_CONFIG.worktree.node_package_name;

    let is_in_package_json =
      package_json.is_ok_and(|json| package_json_declares_dependency(&json, node_package_name));
    let is_in_deno_json =
      deno_json.is_ok_and(|json| deno_json_declares_import(&json, node_package_name));

    is_in_package_json || is_in_deno_json
  }
}

fn package_json_declares_dependency(package_json: &Value, package_name: &str) -> bool {
  !package_json["dependencies"][package_name].is_null()
    || !package_json["devDependencies"][package_name].is_null()
}

fn deno_json_declares_import(deno_json: &Value, package_name: &str) -> bool {
  !deno_json["imports"][package_name].is_null()
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
    let command = self.language_server_binary_path(language_server_id, worktree)?;
    let args = self.language_server_arguments(language_server_id, worktree)?;

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

    assert!(error.contains("Unsupported architecture: x86"));
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
  }
}
