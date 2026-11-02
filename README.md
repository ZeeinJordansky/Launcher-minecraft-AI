# Minty Launcher 2.1.0

Minty Launcher is a Windows x64 Minecraft Java launcher with a custom Avalonia UI, C# services, and Java used only to run the game. The 2.1.0 work replaces the earlier Go/Rust implementation and enables Native AOT publishing.

## Features

- Original dark Minty interface with custom profile avatar, responsive navigation, and animated controls.
- Mojang release/snapshot catalog, installation, hash-checked game assets, and Java runtime setup.
- Microsoft Device Code sign-in with Minecraft ownership checks and refresh-token storage in Windows Credential Manager.
- Fabric, Quilt, Forge, and NeoForge profile installation.
- Modrinth mod search/download with SHA-1/SHA-512 verification.
- Isolated profiles, safe world ZIP import/export, mod management, PNG skin preview, Bedrock launch shortcut, and crash diagnostics.
- ECDSA-signed update manifest and SHA-256 installer verification. Unsigned or invalid updates are rejected.

## Build and test

Requirements: .NET 10 SDK, Windows 10/11 x64 for the release build, Python/Pillow for the application icon, and Inno Setup 6 to produce the installer.

```powershell
dotnet test minty-launcher.Tests/MintyLauncher.Tests.csproj --configuration Release
python -m pip install Pillow
python scripts/installer/build_icon.py
dotnet publish minty-launcher/MintyLauncher.csproj --configuration Release --runtime win-x64 --self-contained true --output dist/launcher
```

The GitHub release workflow additionally requires the repository Actions secret `UPDATE_SIGNING_PRIVATE_KEY`. See [the build guide](./minty-launcher/docs/BUILDING.md) and [security policy](./minty-launcher/SECURITY.md) for signing setup. Release publication must not proceed without it.

## Source layout

- [`minty-launcher/`](./minty-launcher/) — Avalonia UI, C# launcher services, and Java game runtime integration.
- [`minty-launcher.Tests/`](./minty-launcher.Tests/) — unit tests.
- [`.github/workflows/`](./.github/workflows/) — Windows CI and signed release pipeline.
