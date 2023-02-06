use std::ffi::OsString;
use std::thread;
use std::thread::JoinHandle;
use tao::event_loop::{ControlFlow, EventLoop};
use anyhow::Result;
use tao::event::{Event, TrayEvent};
use tao::menu::{ContextMenu, MenuItemAttributes};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::platform::windows::{EventLoopExtWindows, IconExtWindows};
use tao::system_tray::{Icon, SystemTrayBuilder};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;
use winreg::types::FromRegValue;
use crate::config::Config;
use crate::CustomEvent;
use crate::utils::{LogResultExt, show_message_box};

pub struct TrayHandle(JoinHandle<()>);

impl TrayHandle {

    pub fn wait_for_end(self) {
        self.0.join().expect("Can not join with tray icon thread")
    }

}

pub fn create_system_tray(event_loop: &EventLoop<CustomEvent>) -> Result<TrayHandle> {
    let proxy = event_loop.create_proxy();

    let mut auto_start = false;

    let mut tray_menu = ContextMenu::new();
    let _version_item = tray_menu.add_item(MenuItemAttributes::new(concat!("Display Peek (version ", env!("CARGO_PKG_VERSION"), ")"))
        .with_enabled(false));
    let config_item = tray_menu.add_item(MenuItemAttributes::new("Open Config"));
    let mut auto_start_item = tray_menu.add_item(MenuItemAttributes::new("Run at Startup")
        .with_selected(auto_start));
    let quit_item = tray_menu.add_item(MenuItemAttributes::new("Quit"));
    let tray_builder = SystemTrayBuilder::new(Icon::from_resource(32512, None).unwrap(), Some(tray_menu))
        .with_tooltip("Display Peek");

    let handle = thread::spawn(move|| {
        let mut tray_loop: EventLoop<()> = EventLoop::new_any_thread();
        let _tray = tray_builder
            .build(&tray_loop)
            .expect("Can not build system tray");
        tray_loop.run_return(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            match event {
                Event::TrayEvent { event: TrayEvent::RightClick, ..} => {
                    auto_start = is_auto_start_enabled()
                        .log_ok("can not query registry")
                        .unwrap_or(false);
                    auto_start_item.set_selected(auto_start);
                }
                Event::MenuEvent { menu_id, .. } => {
                    if menu_id == quit_item.clone().id() {
                        proxy.send_event(CustomEvent::QuitButton)
                            .log_ok("Main event loop seems to be gone");
                        *control_flow = ControlFlow::Exit;
                    }
                    if menu_id == config_item.clone().id() {
                        if let Err(err) = open::that(Config::path()) {
                            log::warn!("Can not open editor: {}", err);
                            show_message_box("Error", format!("Can not open editor\n{}", err));
                        }
                    }
                    if menu_id == auto_start_item.clone().id() {
                        if auto_start {
                            disable_auto_start()
                                .log_ok("Can not delete registry key");
                        } else {
                            enable_auto_start()
                                .log_ok("Can not create registry key");
                        }
                    }
                }
                _ => {}
            }
        });
        log::trace!("Quiting tray loop");
    });
    Ok(TrayHandle(handle))
}

fn auto_start_directory() -> Result<RegKey> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")?;
    Ok(key)
}

fn is_auto_start_enabled() -> Result<bool> {
    let exe_dir = std::env::current_exe()?
        .canonicalize()?
        .into_os_string();
    let result = auto_start_directory()?
        .enum_values()
        .filter_map(|r| r.log_ok("Problem enumerating registry key"))
        .any(|(key, value)|
            key.eq("DisplayPeek") &&
                OsString::from_reg_value(&value)
                    .log_ok("Can not decode registry value")
                    .map(|v| v.eq(&exe_dir))
                    .unwrap_or(false));
    Ok(result)
}

fn enable_auto_start() -> Result<()> {
    let key = auto_start_directory()?;
    let exe_dir = std::env::current_exe()?
        .canonicalize()?;
    key.set_value("DisplayPeek", &exe_dir.as_os_str())?;
    Ok(())
}

fn disable_auto_start() -> Result<()> {
    let key = auto_start_directory()?;
    key.delete_value("DisplayPeek")?;
    Ok(())
}