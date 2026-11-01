# Minty Launcher

**Download for Windows:** [minty-launcher.exe — latest GitHub Release](https://github.com/ZeeinJordansky/Launcher-minecraft-AI/releases/latest/download/minty-launcher.exe)

Download the single EXE and launch it with a double-click. The user does not need to open a terminal or install Rust, a JDK or a separate Java Runtime. Internet access is required for Minecraft and Java downloads. A compatible Java Runtime is downloaded by the launcher when needed.

## Open-source source code

The complete readable project is in [minty-launcher/](https://github.com/ZeeinJordansky/Launcher-minecraft-AI/tree/main/minty-launcher): Rust source, Java bootstrap, build script, Cargo manifest, documentation and security notes.

GitHub automatically adds “Source code (zip)” and “Source code (tar.gz)” snapshot links to every Release. Those files are source snapshots, not the application. The actual app download is the separate `minty-launcher.exe` asset.

## Features

- Minecraft Java Edition Vanilla release selection and installation.
- Download and SHA-1 integrity validation of official client JAR, libraries, native libraries and assets.
- Automatic install of the Java Runtime required by the selected Minecraft version.
- Microsoft Device Code sign-in with Xbox/XSTS/Minecraft profile validation. Online sign-in requires a Microsoft OAuth Client ID configured in Settings.
- Offline profile for single-player.
- Skin Studio: PNG import, pixel drawing/editing and 64×64 export; legacy 64×32 import is supported.
- “Слабый ПК” mode: lower render and simulation distance, fewer particles, 2 GB Java memory cap and conservative GC parameters.
- Custom Minecraft installation directory.
- Launcher settings and event journal.
- Dark UI with mint accents.

## Important limitations

Skin Studio edits and exports a local PNG. Vanilla Minecraft does not automatically apply a local PNG to an offline profile; displaying it in-game requires a compatible local-skin mod. Online skins remain managed by the official Minecraft account.

The “Слабый ПК” profile uses more conservative settings but cannot guarantee a fixed FPS increase; results depend on hardware, driver, game version and world. The first release supports Vanilla only. Fabric, Forge and NeoForge are not integrated. SHA-1 validation protects official downloaded files; it is not an anti-cheat.

## Build

The Windows workflow tests and compiles `minty-launcher.exe` for Windows x64. JDK 21 is required on the build runner only to compile the small Java bootstrap class embedded into the executable. End users do not need a JDK. A compatible Java Runtime is downloaded automatically by the launcher at game launch.
