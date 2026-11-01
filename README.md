# Minty Launcher

**Windows download:** [minty-launcher.exe — latest GitHub Release](https://github.com/ZeeinJordansky/Launcher-minecraft-AI/releases/latest/download/minty-launcher.exe)

Download that single EXE and launch it with a double-click. No terminal commands, Rust toolchain or separately installed JDK are required from the user. Internet access is required to download Minecraft files and, when needed, a compatible Java Runtime.

## Source code

The readable Rust and Java project is published in the [minty-launcher/ source directory](https://github.com/ZeeinJordansky/Launcher-minecraft-AI/tree/main/minty-launcher). A workflow synchronizes that directory from the committed source snapshot. The application executable is published separately as the `minty-launcher.exe` asset in Releases.

GitHub also generates “Source code (zip)” and “Source code (tar.gz)” links for each tag automatically. These are source snapshots, not the app. To run the launcher, download `minty-launcher.exe`.

## Launcher features

- Install and launch official Minecraft Java Edition Vanilla releases.
- Download and integrity-check official client JAR, libraries, native libraries and assets.
- Automatically install the Java Runtime required by the selected Minecraft version.
- Microsoft Device Code sign-in with Xbox/XSTS/Minecraft profile validation. Online sign-in requires a Microsoft OAuth Client ID configured in settings.
- Offline profile for single-player use.
- Skin Studio: import, edit pixels, draw, and export 64×64 PNG skins; legacy 64×32 import is supported.
- “Слабый ПК” mode: lower render/simulation distance, fewer particles, reduced Java memory allocation, and gentler garbage-collector settings.
- Custom Minecraft installation directory.
- Launcher settings and event journal.
- Dark interface with mint accents.

## Limitations to know

Skin Studio edits and exports a local PNG. Vanilla Minecraft does not automatically apply a local PNG to an offline profile; displaying it in-game requires a compatible local-skin mod. Online skins are managed by the official Minecraft account.

The “Слабый ПК” mode applies conservative settings, but no launcher can guarantee a fixed FPS increase across all hardware, drivers and worlds. The first release supports Vanilla only; Fabric, Forge and NeoForge are not integrated. SHA-1 checks protect the integrity of supported Mojang downloads and are not an anti-cheat system.

## Build

GitHub Actions builds the Windows x64 binary and publishes it after tests pass. JDK 21 is needed only on the build runner to compile the embedded Java bootstrap class. End users do not need JDK installed; the launcher downloads the required Java Runtime when a Minecraft version is launched.
