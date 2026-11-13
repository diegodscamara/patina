//! Small preferences kept between launches, in the app's config folder.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Playback volume, 0.0 to 1.0.
    pub volume: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { volume: 0.8 }
    }
}

fn path() -> Option<PathBuf> {
    directories::ProjectDirs::from("dev", "diegodscamara", "Patina")
        .map(|dirs| dirs.config_dir().join("settings.toml"))
}

impl Settings {
    pub fn load() -> Self {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = path() else { return };
        let result = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| {
                let text = toml::to_string(self).unwrap_or_default();
                std::fs::write(&path, text)
            });
        if let Err(e) = result {
            tracing::warn!("saving settings failed: {e}");
        }
    }
}
