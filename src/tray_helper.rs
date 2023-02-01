use std::thread;
use std::thread::JoinHandle;
use tao::event_loop::{ControlFlow, EventLoop};
use anyhow::Result;
use tao::event::Event;
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

    let mut tray_menu = ContextMenu::new();
    let _version_item = tray_menu.add_item(MenuItemAttributes::new(concat!("Display Peek (version ", env!("CARGO_PKG_VERSION"), ")"))
        .with_enabled(false));
    let config_item = tray_menu.add_item(MenuItemAttributes::new("Open Config"));
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
                Event::MenuEvent { menu_id, .. } => {
                    if menu_id == quit_item.clone().id() {
                        if proxy.send_event(CustomEvent::QuitButton).is_err() {
                            log::warn!("Main event loop seems to be gone");
                        }
                        *control_flow = ControlFlow::Exit;
                    }
                    if menu_id == config_item.clone().id() {
                        if let Err(err) = open::that(Config::path()) {
                            log::warn!("Can not open editor: {}", err);
                            show_message_box("Error", format!("Can not open editor\n{}", err));
                        }
                    }
                },
                _ => {}
            }
        });
        log::trace!("Quiting tray loop");
    });
    Ok(TrayHandle(handle))
}