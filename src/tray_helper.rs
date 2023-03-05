use std::thread;
use std::thread::JoinHandle;
use tao::event_loop::{ControlFlow, EventLoop};
use anyhow::Result;
use error_tools::log::LogResultExt;
use tao::event::{Event, TrayEvent};
use tao::menu::{ContextMenu, MenuItemAttributes};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::platform::windows::{EventLoopExtWindows, IconExtWindows};
use tao::system_tray::{Icon, SystemTrayBuilder};
use crate::config::Config;
use crate::CustomEvent;
use crate::utils::show_message_box;

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
                    auto_start = autostart::is_enabled()
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
                            autostart::disable()
                                .log_ok("Can not delete registry key");
                        } else {
                            autostart::enable()
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

mod autostart {
    use std::ffi::OsString;
    use anyhow::Result;
    use error_tools::log::LogResultExt;
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    use winreg::types::FromRegValue;

    fn directory() -> Result<RegKey> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu.create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")?;
        Ok(key)
    }

    fn reg_key() -> &'static str  {
        "DisplayPeek"
    }

    fn start_cmd() -> Result<OsString> {
        let mut cmd = OsString::from("\"");
        let exe_dir = dunce::canonicalize(std::env::current_exe()?)?;
        cmd.push(exe_dir);
        cmd.push("\"");
        Ok(cmd)
    }

    pub fn is_enabled() -> Result<bool> {
        let cmd = start_cmd()?;
        let result = directory()?
            .enum_values()
            .filter_map(|r| r.log_ok("Problem enumerating registry key"))
            .any(|(key, value)|
                key.eq(reg_key()) &&
                    OsString::from_reg_value(&value)
                        .log_ok("Can not decode registry value")
                        .map(|v| v.eq(&cmd))
                        .unwrap_or(false));
        Ok(result)
    }

    pub fn enable() -> Result<()> {
        let key = directory()?;
        let cmd = start_cmd()?;
        key.set_value(reg_key(), &cmd)?;
        Ok(())
    }

    pub fn disable() -> Result<()> {
        let key = directory()?;
        key.delete_value(reg_key())?;
        Ok(())
    }
}
