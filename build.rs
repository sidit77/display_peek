#[cfg(windows)]
fn main() {
    let mut res = tauri_winres::WindowsResource::new();
    res.set_icon("icon.ico");
    res.compile().unwrap();
}

#[cfg(unix)]
fn main() {}