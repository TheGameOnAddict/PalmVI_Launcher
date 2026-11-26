# Mandyhub

A community launcher for VIctory-based projects.

[Download Mandyhub for Windows](https://github.com/TheGameOnAddict/mandyhub/releases/latest).
In **Settings → Launcher updates**, check for a newer release and download it.
Mandyhub checks the executable against GitHub’s SHA-256 checksum. When the
download is ready, close the current launcher and open the downloaded copy.

**Play** opens the save picker. Continue a save, start fresh, copy an earlier
save or a save from another build, or launch from a favorite checkpoint. To
keep a reusable checkpoint, select a save, click **Add to favorites**, and name
it. Each checkpoint launch uses a copy.

## Screenshots

These screenshots use example projects, saves, and performance figures.

![Mandyhub project overview with example projects, install status, and profile tabs](docs/screenshots/project-profile.png)

![Mandyhub save picker with an example project selected](docs/screenshots/save-picker.png)

- **Projects from PortWatch:** the list of public projects, live, with their reported frame rate, the graphics cards they were made for, and whether PortWatch has verified them.
- **Custom repo links:** add any GitHub repo by its link.
- **Install once, play many times:** a project is downloaded and built on your PC once and stays installed.
- **Updates:** installed projects are checked for new versions at start and every 15 minutes; an update can be installed now or later.
- **Install folder:** choose where projects are downloaded and built (a big drive is best); installed projects can be moved there.
- **Uninstall:** removes a project's download, build and shader cache (saves stay).
- **Game files check:** Play stays off until the folder with your own game files is chosen.
- **Fullscreen:** optional borderless fullscreen for the game window.
- **Project profiles:** keep graphics, launch arguments, environment switches, and private notes for each build.
- **Build tracking:** see when you last ran a build and spot installs and updates you have yet to try.
- **Compatibility:** build forks with a nested Cargo workspace and use shader warm-up on Scarlett builds that provide the `shaders` command.

No game files are included or downloaded. You need your own copy of the game.

## Requirements

- Windows 10 or 11, 64-bit
- To build projects: Git, Rust and the C++ build tools. Mandyhub finds them where their installers put them, and **Install missing tools for me** installs what is missing with winget (Windows asks for permission).

## Build

```powershell
cargo build --release
```

The launcher is `target\release\mandyhub.exe`.

## Backgrounds

Put your own `.jpg` or `.png` pictures in `%LOCALAPPDATA%\Mandyhub\backgrounds`. They slowly move and fade into each other in a banner across the top of each project's page. Without pictures, the page has no banner.

## Fonts

The display type is Josefin Sans and Big Shoulders Display, built into the exe from `fonts/` under the SIL Open Font License (license texts in `fonts/`). Reading text and icons use Windows' own Segoe UI and Segoe Fluent Icons.

## Credits

Mandyhub by **Zettrox**. Project list from [PortWatch](https://portwatch-vi.xyz). The projects are their authors' work, built on VIctory by Marvisak and its contributors.

- [ellion369](https://github.com/ellion369) designed and implemented the launcher redesign.
- [TheGameOnAddict](https://github.com/TheGameOnAddict) contributed the save picker and named checkpoints, project settings and notes, build tracking, compatibility fixes, and release updater.
