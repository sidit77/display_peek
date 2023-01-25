use std::thread;
use tray_icon::menu::MenuEvent;
use winit::event_loop::EventLoop;
use crate::CustomEvent;

pub fn forward_events(event_loop: &EventLoop<CustomEvent>){
    let proxy = event_loop.create_proxy();
    let receiver = MenuEvent::receiver();
    thread::spawn(move || {
       loop {
           if let Ok(event) = receiver.recv() {
               if let Err(_) = proxy.send_event(CustomEvent::Menu(event.id)) {
                   break;
               }
           }
       }
    });
}