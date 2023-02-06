use std::path::PathBuf;
use directories_next::BaseDirs;
use notify::{RecommendedWatcher, Watcher, RecursiveMode};
use serde::Deserialize;
use tao::dpi::{LogicalPosition, LogicalSize};
use tao::event_loop::EventLoop;
use anyhow::Result;
use crate::CustomEvent;
use crate::utils::LogResultExt;

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

#[must_use]
pub struct ConfigWatcher(RecommendedWatcher);

impl Config {

    pub fn path() -> PathBuf {
        let dirs = BaseDirs::new().expect("can not get directories");
        let config_dir = dirs.config_dir();
        config_dir.join("DisplayPeek.toml")
    }

    pub fn create_watcher(event_loop: &EventLoop<CustomEvent>) -> Result<ConfigWatcher> {
        let proxy = event_loop.create_proxy();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            match res {
                Ok(event) => {
                    if event.kind.is_modify() {
                        proxy.send_event(CustomEvent::ConfigChange)
                            .log_ok("Cannot send config change event to eventloop");
                    }
                },
                Err(e) => log::warn!("watch error: {:?}", e),
            };
        })?;
        watcher.watch(&Self::path(), RecursiveMode::NonRecursive)?;
        Ok(ConfigWatcher(watcher))
    }

    pub fn load() -> Result<Config> {
        if !Self::path().exists(){
            log::info!("Writing default config");
            std::fs::write(Self::path(), include_bytes!("../resources/default_config.toml"))?;
        }
        let config: Config = toml::from_str(&std::fs::read_to_string(Self::path())?)?;
        Ok(config)
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
                .and_then(|x|x.position)
                .unwrap_or(self.position),
            size: overlay_override
                .and_then(|x|x.size)
                .unwrap_or(self.size),
        }
    }
}