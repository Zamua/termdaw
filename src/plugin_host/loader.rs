//! Plugin loader abstraction for testability and future format support
//!
//! This module provides a trait-based abstraction over plugin loading,
//! allowing for mock implementations in tests and potential support
//! for multiple plugin formats (VST3, AU) in the future.

use std::path::Path;

use super::{ActivePluginProcessor, PluginHost, PluginInfo};

/// Error type for plugin loading failures
#[derive(Debug, thiserror::Error)]
pub enum PluginLoadError {
    /// Failed to load the plugin bundle
    #[error("Failed to load plugin: {0}")]
    LoadFailed(String),
    /// Failed to activate the plugin
    #[error("Failed to activate plugin: {0}")]
    ActivationFailed(String),
}

/// Result of successful plugin loading
pub struct LoadedPlugin {
    /// The active processor ready for the audio thread
    pub processor: ActivePluginProcessor,
    /// Plugin metadata
    pub info: PluginInfo,
}

/// Trait for loading plugins - enables mocking in tests
pub trait PluginLoader: Send + Sync {
    /// Load and activate a plugin, returning a ready-to-use processor
    fn load_plugin(
        &self,
        path: &Path,
        sample_rate: f64,
        buffer_size: u32,
    ) -> Result<LoadedPlugin, PluginLoadError>;
}

/// Default CLAP plugin loader using clack-host
pub struct ClapPluginLoader;

impl PluginLoader for ClapPluginLoader {
    fn load_plugin(
        &self,
        path: &Path,
        sample_rate: f64,
        buffer_size: u32,
    ) -> Result<LoadedPlugin, PluginLoadError> {
        // Load the plugin
        let mut host = PluginHost::load(path, sample_rate, buffer_size)
            .map_err(PluginLoadError::LoadFailed)?;

        // Get info before activation
        let info = host.info().clone();

        // Activate and get the processor
        let processor = host.activate().map_err(PluginLoadError::ActivationFailed)?;

        Ok(LoadedPlugin { processor, info })
    }
}

impl Default for ClapPluginLoader {
    fn default() -> Self {
        Self
    }
}

#[cfg(test)]
pub mod mock {
    use super::*;

    /// Mock plugin loader for testing
    pub struct MockPluginLoader {
        /// If true, load_plugin will return an error
        pub should_fail: bool,
        /// Error message to return on failure
        pub error_message: String,
    }

    impl MockPluginLoader {
        /// Create a mock loader that succeeds
        pub fn new() -> Self {
            Self {
                should_fail: false,
                error_message: "Mock failure".to_string(),
            }
        }

        /// Create a mock loader that fails
        pub fn failing(message: &str) -> Self {
            Self {
                should_fail: true,
                error_message: message.to_string(),
            }
        }
    }

    impl Default for MockPluginLoader {
        fn default() -> Self {
            Self::new()
        }
    }

    impl PluginLoader for MockPluginLoader {
        fn load_plugin(
            &self,
            path: &Path,
            _sample_rate: f64,
            _buffer_size: u32,
        ) -> Result<LoadedPlugin, PluginLoadError> {
            if self.should_fail {
                Err(PluginLoadError::LoadFailed(self.error_message.clone()))
            } else {
                // Cannot create a real ActivePluginProcessor without a real plugin
                // Tests using MockPluginLoader should not actually call this in a way
                // that expects a working processor - they should mock at a higher level
                // or use integration tests with real plugins
                Err(PluginLoadError::LoadFailed(format!(
                    "MockPluginLoader cannot create real processors (path: {:?})",
                    path
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_loader_failing() {
        let loader = mock::MockPluginLoader::failing("test error");
        let result = loader.load_plugin(Path::new("/fake/path.clap"), 44100.0, 512);
        assert!(result.is_err());
        match result {
            Err(PluginLoadError::LoadFailed(msg)) => {
                assert_eq!(msg, "test error");
            }
            _ => panic!("Expected LoadFailed error"),
        }
    }

    #[test]
    fn test_plugin_load_error_display() {
        let err = PluginLoadError::LoadFailed("bundle not found".to_string());
        assert_eq!(err.to_string(), "Failed to load plugin: bundle not found");

        let err = PluginLoadError::ActivationFailed("audio config rejected".to_string());
        assert_eq!(
            err.to_string(),
            "Failed to activate plugin: audio config rejected"
        );
    }
}
