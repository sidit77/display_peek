use std::path::PathBuf;
use directories_next::BaseDirs;
use serde::Deserialize;
use tao::dpi::{LogicalPosition, LogicalSize};

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct OverlayConfig {
    pub position: LogicalPosition<f64>,
    pub size: LogicalSize<f64>
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct OverlayOverride {
    pub position: Option<LogicalPosition<f64>>,
    pub size: Option<LogicalSize<f64>>
}


#[derive(Debug, Clone, Deserialize)]
pub struct MonitorConfig {
    pub name: String,
    pub overlay: Option<OverlayOverride>
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub overlay: OverlayConfig,
    pub monitors: Vec<MonitorConfig>
}

impl Config {

    pub fn path() -> PathBuf {
        let dirs = BaseDirs::new().expect("can not get directories");
        let config_dir = dirs.config_dir();
        config_dir.join("DisplayPeek.toml")
    }

    pub fn load() -> Config {
        let config: Config = toml::from_str(&std::fs::read_to_string(Self::path()).unwrap()).unwrap();
        config
    }

    pub fn get_overlay_config(&self, monitor_name: &str) -> Option<OverlayConfig> {
        self.monitors
            .iter()
            .find(|m|m.name == monitor_name)
            .map(|c| self.overlay.with_override(c.overlay))
    }

}

impl OverlayConfig {
    pub fn with_override(self, overlay_override: Option<OverlayOverride>) -> Self {
        Self {
            position: overlay_override
                .map(|x|x.position)
                .flatten()
                .unwrap_or(self.position),
            size: overlay_override
                .map(|x|x.size)
                .flatten()
                .unwrap_or(self.size),
        }
    }
}