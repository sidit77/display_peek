use std::sync::mpsc::{Sender, TryRecvError};
use tao::event_loop::EventLoop;
use crate::CustomEvent;
use crate::directx::Display;

#[derive(Debug, Clone)]
pub struct VSyncThreadHandle(Sender<Option<Display>>);

impl VSyncThreadHandle {
    pub fn change_display(&self, display: impl Into<Option<Display>>) {
        if self.0.send(display.into()).is_err() {
            log::warn!("Cannot set display for vsync thread");
        }
    }
}

pub fn start_vsync_thread(event_loop: &EventLoop<CustomEvent>, display: impl Into<Option<Display>>) -> VSyncThreadHandle {
    let (tx, rx) = std::sync::mpsc::channel::<Option<Display>>();
    let proxy = event_loop.create_proxy();
    let mut current_display: Option<Display> = display.into();
    std::thread::spawn(move || {
        loop {
            current_display = match current_display.take() {
                None => match rx.recv() {
                    Ok(new_display) => {
                        log::trace!("Switching vsync thread to {:?}", new_display.clone().map(|d|d.name()));
                        new_display
                    },
                    Err(_) => break
                }
                Some(display) => match rx.try_recv() {
                    Ok(new_display) => {
                        log::trace!("Switching vsync thread to {:?}", new_display.clone().map(|d|d.name()));
                        new_display
                    },
                    Err(TryRecvError::Disconnected) => break,
                    Err(TryRecvError::Empty) => {
                        if let Err(e) = display.wait_for_vsync(){
                            log::warn!("vsync error: {:?}", e);
                        }
                        match proxy.send_event(CustomEvent::VBlank) {
                            Ok(_) => Some(display),
                            Err(_) => break
                        }
                    }
                }
            }
        }
        log::trace!("Stopping vsync thread");
    });
    VSyncThreadHandle(tx)
}