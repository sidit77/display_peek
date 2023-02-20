# Display Peek

## Overview
A simple app for Windows that mirrors a screen onto another every time the mouse enters it. This is very useful for multi-monitor setups where one of the monitors is not easily visible (projector setup, source switched, etc.).

## Demo
https://user-images.githubusercontent.com/5053369/215463529-d08d5a3a-f40f-48fd-a0be-335f8451f5cb.mp4

## Installation
The program is distributed as a single binary, no installation is necessary.
### Pre-built Binaries
You can download pre-built binaries directly from the release section:

[**Download**](https://github.com/sidit77/display_peek/releases/)
### Building from Source
Simply clone the repository and build using cargo:
```bash
git clone https://github.com/sidit77/display_peek.git
cd display_peek
cargo build --release
```

## Configuration
This app is configured using its config file. Simply right click the tray icon and click `Open Config`. The app will automatically reload the config everytime you save.

## Limitations
Apps running as administator can block to cursor tracking als long as they are focused unless this app is also running as administrator.

## License
MIT License
