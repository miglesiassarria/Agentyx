# Icons

Tauri requires the following icon files in this directory for
production builds. In v0.1 the build will fail without them; in
dev mode (`cargo tauri dev`) icons are not strictly required.

Generate them with `cargo tauri icon path/to/source.png` (the
`tauri` CLI resizes a 1024×1024 PNG into all required formats).

Required filenames (referenced from `tauri.conf.json`):

- `32x32.png`
- `128x128.png`
- `128x128@2x.png` (256×256)
- `icon.icns` (macOS)
- `icon.ico` (Windows)
- `icon.png` (Linux, optional but recommended)

A placeholder source icon can be added to `assets/` and run
through `tauri icon` to populate this directory.
