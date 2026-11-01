# Minty Launcher

**Windows download:** [minty-launcher.exe — latest GitHub Release](https://github.com/ZeeinJordansky/Launcher-minecraft-AI/releases/latest/download/minty-launcher.exe)

Download that single EXE and launch it with a double-click. The app does not require the user to open a terminal, install Rust, install a JDK, or manually configure Java. A compatible Java Runtime is downloaded automatically by the launcher when needed for the selected Minecraft version.

## What is in the launcher

- Minecraft Java Edition Vanilla release selection and installation.
- Verified downloads for official client JAR, libraries, natives and assets.
- Automatic Java Runtime installation for the selected Minecraft version.
- Microsoft Device Code sign-in, with Xbox/XSTS/Minecraft profile validation. Online sign-in requires a Microsoft OAuth Client ID configured in launcher settings.
- Offline profile for single-player use.
- Skin Studio: import, draw/edit pixels and export 64×64 PNG skins (legacy 64×32 import is supported).
- Optional “Слабый ПК” profile: conservative graphics options, lower render/simulation distances, fewer particles, capped FPS and a lower Java memory target.
- Custom Minecraft game directory selection.
- Settings and launcher event log.
- Dark interface with mint accents.

## Important notes

Skin Studio edits and exports a local PNG. Vanilla Minecraft does not automatically apply a local PNG to an offline profile; displaying that skin in-game requires a compatible local-skin mod. Online skins remain managed by the official Minecraft account.

The “Слабый ПК” profile changes game settings and launch parameters, but cannot guarantee a specific FPS increase. The result depends on hardware, Minecraft version, drivers and the world being played.

The first release supports Vanilla. Fabric, Forge and NeoForge are not yet integrated. File SHA-1 checks protect the integrity of supported official downloads; they are not an anti-cheat system.

## Open-source code

The source archive and project documentation are stored in this repository. The application binary is published separately as the `minty-launcher.exe` asset in Releases. GitHub also generates “Source code (zip)” and “Source code (tar.gz)” links automatically for each tag; those are repository snapshots, not the launcher executable.

## Build and test

The Windows workflow builds the application for `x86_64-pc-windows-msvc`. JDK 21 is needed only on the build runner to compile the small Java bootstrap class embedded into the executable. End users do not need a JDK. The launcher downloads the required Java Runtime on first launch of a Minecraft version.

Release publication is gated by `cargo test` and a successful Windows release build.
