//! Zed extension entry point for running `dprint lsp`.

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

const DPRINT_REPOSITORY: &str = "dprint/dprint";
const DPRINT_BINARY_NAME: &str = "dprint";
const RELEASE_DIRECTORY_PREFIX: &str = "dprint-";
const NPM_BINARY_PATH: &str = "node_modules/.bin/dprint";
const NPM_BINARY_PATH_WINDOWS: &str = "node_modules/.bin/dprint.cmd";

#[derive(Default)]
struct DprintExtension {
  cached_auto_install_path: Option<String>,
}

struct DprintRelease {
  release: GithubRelease,
  os: Os,
  architecture: Architecture,
  directory: String,
  binary_path: PathBuf,
}

impl DprintRelease {
  fn latest() -> zed::Result<Self> {
    let release = zed::latest_github_release(
      DPRINT_REPOSITORY,
      GithubReleaseOptions {
        require_assets: true,
        pre_release: false,
      },
    )?;
    let (os, architecture) = zed::current_platform();
    let directory = release_directory(&release.version);
    let binary_path = release_binary_path(&release.version, os);

    Ok(Self {
      release,
      os,
      architecture,
      directory,
      binary_path,
    })
  }

  fn ensure_installed(&self, language_server_id: &LanguageServerId) -> zed::Result<&Path> {
    if !self.binary_path.is_file() {
      zed::set_language_server_installation_status(
        language_server_id,
        &LanguageServerInstallationStatus::Downloading,
      );

      let asset_name = release_asset_name(self.architecture, self.os)?;
      let asset = self
        .release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .ok_or_else(|| {
          format!(
            "dprint release {} has no {asset_name} asset",
            self.release.version
          )
        })?;

      zed::download_file(
        &asset.download_url,
        &self.directory,
        DownloadedFileType::Zip,
      )
      .map_err(|error| {
        format!(
          "failed to download dprint {}: {error}",
          self.release.version
        )
      })?;
    }

    if !self.binary_path.is_file() {
      return Err(format!(
        "dprint {} was downloaded but its executable was not found at {:?}",
        self.release.version, self.binary_path
      ));
    }

    if self.os != Os::Windows {
      zed::make_file_executable(&path_to_command(&self.binary_path)?)?;
    }

    self.cleanup_old_releases();
    Ok(&self.binary_path)
  }

  fn cleanup_old_releases(&self) {
    let Ok(entries) = fs::read_dir(".") else {
      return;
    };

    for entry in entries.flatten() {
      let path = entry.path();
      let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        continue;
      };

      if name == self.directory || !name.starts_with(RELEASE_DIRECTORY_PREFIX) {
        continue;
      }

      if path.is_dir() {
        let _ = fs::remove_dir_all(path);
      } else {
        let _ = fs::remove_file(path);
      }
    }
  }
}

impl DprintExtension {
  fn resolve_language_server_binary(
    &mut self,
    language_server_id: &LanguageServerId,
    worktree: &Worktree,
    configured_path: Option<String>,
  ) -> zed::Result<String> {
    if let Some(path) = configured_path {
      return Ok(path);
    }

    let (os, _) = zed::current_platform();
    if worktree_declares_dprint_dependency(worktree) {
      return path_to_command(
        &Path::new(&worktree.root_path()).join(worktree_dprint_binary_path(os)),
      );
    }

    if let Some(path) = worktree.which(DPRINT_BINARY_NAME) {
      return Ok(path);
    }

    if let Some(path) = self
      .cached_auto_install_path
      .as_ref()
      .filter(|path| Path::new(path).is_file())
    {
      return Ok(path.clone());
    }

    self.install_latest_release(language_server_id)
  }

  fn install_latest_release(
    &mut self,
    language_server_id: &LanguageServerId,
  ) -> zed::Result<String> {
    zed::set_language_server_installation_status(
      language_server_id,
      &LanguageServerInstallationStatus::CheckingForUpdate,
    );

    let result = DprintRelease::latest().and_then(|release| {
      let path = path_to_command(release.ensure_installed(language_server_id)?)?;
      Ok(path)
    });

    match result {
      Ok(path) => {
        zed::set_language_server_installation_status(
          language_server_id,
          &LanguageServerInstallationStatus::None,
        );
        self.cached_auto_install_path = Some(path.clone());
        Ok(path)
      }
      Err(error) => {
        zed::set_language_server_installation_status(
          language_server_id,
          &LanguageServerInstallationStatus::Failed(error.clone()),
        );
        Err(error)
      }
    }
  }
}

impl zed::Extension for DprintExtension {
  fn new() -> Self {
    Self::default()
  }

  fn language_server_command(
    &mut self,
    language_server_id: &LanguageServerId,
    worktree: &Worktree,
  ) -> zed::Result<zed::Command> {
    let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)?;
    let (configured_path, args, env) = settings.binary.map_or_else(
      || (None, default_language_server_args(), Vec::new()),
      |binary| {
        (
          binary.path,
          binary
            .arguments
            .unwrap_or_else(default_language_server_args),
          binary.env.unwrap_or_default().into_iter().collect(),
        )
      },
    );

    Ok(zed::Command {
      command: self.resolve_language_server_binary(
        language_server_id,
        worktree,
        configured_path,
      )?,
      args,
      env,
    })
  }

  fn language_server_initialization_options(
    &mut self,
    language_server_id: &LanguageServerId,
    worktree: &Worktree,
  ) -> zed::Result<Option<Value>> {
    Ok(LspSettings::for_worktree(language_server_id.as_ref(), worktree)?.initialization_options)
  }

  fn language_server_workspace_configuration(
    &mut self,
    language_server_id: &LanguageServerId,
    worktree: &Worktree,
  ) -> zed::Result<Option<Value>> {
    Ok(LspSettings::for_worktree(language_server_id.as_ref(), worktree)?.settings)
  }
}

fn default_language_server_args() -> Vec<String> {
  vec!["lsp".into()]
}

fn worktree_dprint_binary_path(os: Os) -> &'static str {
  match os {
    Os::Windows => NPM_BINARY_PATH_WINDOWS,
    Os::Mac | Os::Linux => NPM_BINARY_PATH,
  }
}

fn worktree_declares_dprint_dependency(worktree: &Worktree) -> bool {
  worktree
    .read_text_file("package.json")
    .ok()
    .and_then(|contents| serde_json::from_str(&contents).ok())
    .is_some_and(|package_json| package_json_declares_dependency(&package_json))
}

fn package_json_declares_dependency(package_json: &Value) -> bool {
  json_object_has_non_null_key(package_json, "dependencies", DPRINT_BINARY_NAME)
    || json_object_has_non_null_key(package_json, "devDependencies", DPRINT_BINARY_NAME)
}

fn json_object_has_non_null_key(json: &Value, object_key: &str, item_key: &str) -> bool {
  json
    .get(object_key)
    .and_then(Value::as_object)
    .and_then(|object| object.get(item_key))
    .is_some_and(|value| !value.is_null())
}

fn release_asset_name(architecture: Architecture, os: Os) -> zed::Result<String> {
  let architecture = match architecture {
    Architecture::X8664 => "x86_64",
    Architecture::Aarch64 => "aarch64",
    Architecture::X86 => {
      return Err("dprint does not publish release binaries for 32-bit x86".into());
    }
  };
  let target = match os {
    Os::Linux => "unknown-linux-gnu",
    Os::Mac => "apple-darwin",
    Os::Windows => "pc-windows-msvc",
  };

  Ok(format!("dprint-{architecture}-{target}.zip"))
}

fn release_directory(version: &str) -> String {
  format!("{RELEASE_DIRECTORY_PREFIX}{version}")
}

fn release_binary_path(version: &str, os: Os) -> PathBuf {
  let file_name = match os {
    Os::Windows => "dprint.exe",
    Os::Mac | Os::Linux => DPRINT_BINARY_NAME,
  };
  PathBuf::from(release_directory(version)).join(file_name)
}

fn path_to_command(path: &Path) -> zed::Result<String> {
  path
    .to_str()
    .map(str::to_owned)
    .ok_or_else(|| format!("dprint executable path is not valid UTF-8: {path:?}"))
}

zed::register_extension!(DprintExtension);

#[cfg(test)]
mod tests {
  use super::*;
  use zed::serde_json::json;

  #[test]
  fn release_assets_match_supported_platforms() {
    assert_eq!(
      release_asset_name(Architecture::X8664, Os::Linux).unwrap(),
      "dprint-x86_64-unknown-linux-gnu.zip"
    );
    assert_eq!(
      release_asset_name(Architecture::Aarch64, Os::Mac).unwrap(),
      "dprint-aarch64-apple-darwin.zip"
    );
    assert_eq!(
      release_asset_name(Architecture::X8664, Os::Windows).unwrap(),
      "dprint-x86_64-pc-windows-msvc.zip"
    );
    assert!(release_asset_name(Architecture::X86, Os::Linux).is_err());
  }

  #[test]
  fn release_binary_paths_match_platforms() {
    assert_eq!(
      release_binary_path("0.55.2", Os::Linux),
      PathBuf::from("dprint-0.55.2/dprint")
    );
    assert_eq!(
      release_binary_path("0.55.2", Os::Windows),
      PathBuf::from("dprint-0.55.2/dprint.exe")
    );
  }

  #[test]
  fn worktree_binary_paths_match_platforms() {
    assert_eq!(worktree_dprint_binary_path(Os::Linux), NPM_BINARY_PATH);
    assert_eq!(
      worktree_dprint_binary_path(Os::Windows),
      NPM_BINARY_PATH_WINDOWS
    );
  }

  #[test]
  fn package_json_detection_checks_regular_and_dev_dependencies() {
    assert!(package_json_declares_dependency(
      &json!({ "dependencies": { "dprint": "1.0.0" } })
    ));
    assert!(package_json_declares_dependency(
      &json!({ "devDependencies": { "dprint": "1.0.0" } })
    ));
    assert!(!package_json_declares_dependency(
      &json!({ "dependencies": { "prettier": "1.0.0" } })
    ));
    assert!(!package_json_declares_dependency(
      &json!({ "dependencies": { "dprint": null } })
    ));
    assert!(!package_json_declares_dependency(
      &json!({ "dependencies": ["dprint"] })
    ));
  }

  #[test]
  fn default_command_runs_dprint_lsp() {
    assert_eq!(default_language_server_args(), ["lsp"]);
  }
}
