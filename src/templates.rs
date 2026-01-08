//! Template management - downloading and installing starter templates
//!
//! On first run, downloads templates from the GitHub release matching the app version.
//! When running from the repo (development), uses local templates directory.

use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

use flate2::read::GzDecoder;
use tar::Archive;

/// App version from Cargo.toml
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// GitHub repo for downloading templates
const GITHUB_REPO: &str = "Zamua/termdaw";

/// Get local templates directory (for development)
fn local_templates_dir() -> PathBuf {
    PathBuf::from("templates")
}

/// Check if local templates exist (development mode)
/// Public so other modules can check dev mode
pub fn local_templates_exist() -> bool {
    let dir = local_templates_dir();
    dir.exists() && dir.join("default").exists()
}

/// Get the installed templates directory path (~/.config/termdaw/templates)
fn installed_templates_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("termdaw")
        .join("templates")
}

/// Get the templates directory path
/// Prefers local templates (development) over installed templates
pub fn templates_dir() -> PathBuf {
    if local_templates_exist() {
        return local_templates_dir();
    }
    installed_templates_dir()
}

/// Get the projects directory path
/// Dev mode: ./ (current directory)
/// Installed: ~/.config/termdaw/projects
pub fn projects_dir() -> PathBuf {
    if local_templates_exist() {
        PathBuf::from(".")
    } else {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("termdaw")
            .join("projects")
    }
}

/// Check if templates are available (local or installed)
pub fn templates_exist() -> bool {
    // Check local first (development), then installed
    if local_templates_exist() {
        return true;
    }
    let dir = installed_templates_dir();
    dir.exists() && dir.join("default").exists()
}

/// Download and install templates from GitHub release
///
/// Returns Ok(true) if templates were downloaded, Ok(false) if already existed
pub fn ensure_templates() -> Result<bool, TemplateError> {
    if templates_exist() {
        return Ok(false);
    }

    println!("Downloading starter templates for v{}...", VERSION);

    let url = format!(
        "https://github.com/{}/releases/download/v{}/templates.tar.gz",
        GITHUB_REPO, VERSION
    );

    // Download the tarball
    let response = ureq::get(&url).call().map_err(|e| match e {
        ureq::Error::Status(404, _) => TemplateError::NotFound(VERSION.to_string()),
        ureq::Error::Status(code, _) => TemplateError::HttpError(code),
        e => TemplateError::Network(e.to_string()),
    })?;

    // Read response body
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(100_000_000) // 100MB limit
        .read_to_end(&mut bytes)
        .map_err(|e| TemplateError::Network(e.to_string()))?;

    println!("Extracting templates...");

    // Create templates directory (always install to config dir, not local)
    let install_dir = installed_templates_dir();
    fs::create_dir_all(&install_dir).map_err(TemplateError::Io)?;

    // Extract tarball
    let decoder = GzDecoder::new(&bytes[..]);
    let mut archive = Archive::new(decoder);
    archive.unpack(&install_dir).map_err(TemplateError::Io)?;

    println!("Templates installed to {:?}", install_dir);

    Ok(true)
}

/// Errors that can occur during template operations
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("Templates not found for version {0} - this version may not have been released yet")]
    NotFound(String),

    #[error("HTTP error: status {0}")]
    HttpError(u16),

    #[error("Network error: {0}")]
    Network(String),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_templates_dir_contains_templates() {
        let dir = templates_dir();
        assert!(dir.to_string_lossy().contains("templates"));
    }

    #[test]
    fn test_version_is_set() {
        assert!(!VERSION.is_empty());
        // Should be semver format
        assert!(VERSION.contains('.'));
    }

    #[test]
    fn test_local_templates_detected() {
        // When running tests from repo, local templates should exist
        assert!(local_templates_exist());
    }

    #[test]
    fn test_templates_exist_finds_local() {
        // templates_exist should return true when local templates are present
        assert!(templates_exist());
    }
}
