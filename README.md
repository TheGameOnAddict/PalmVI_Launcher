# Mandyhub

A community launcher for VIctory-based projects.

- **Projects from PortWatch:** the list of public projects, live, with their reported frame rate, the graphics cards they were made for, and whether PortWatch has verified them.
- **Custom repo links:** add any GitHub repo by its link.
- **Install once, play many times:** a project is downloaded and built on your PC once and stays installed.
- **Updates:** installed projects are checked for new versions at start and every 15 minutes; an update can be installed now or later.
- **Uninstall:** removes a project's download, build and shader cache (saves stay).
- **Game files check:** Play stays off until the folder with your own game files is chosen.
- **Fullscreen:** optional borderless fullscreen for the game window.

No game files are included or downloaded. You need your own copy of the game.

## Requirements

- Windows 10 or 11, 64-bit
- [Git](https://git-scm.com) and [Rust](https://rustup.rs) with the MSVC build tools (to build projects)

## Build

```powershell
cargo build --release
```

The launcher is `target\release\mandyhub.exe`.

## Backgrounds

Put your own `.jpg` or `.png` pictures in `%LOCALAPPDATA%\Mandyhub\backgrounds`. They slowly move and fade into each other behind the launcher. Without pictures, an animated scene is shown.

## Credits

Mandyhub by **Zettrox**. Project list from [PortWatch](https://portwatch-vi.xyz). The projects are their authors' work, built on VIctory by Marvisak and its contributors.
