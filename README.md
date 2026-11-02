# Minty Launcher 2.2.0

Minty Launcher is a Windows x64 Minecraft Java launcher with a custom Avalonia interface, C# launcher services, and Java used only to run Minecraft.

## Features

- Gray interface with a customizable accent color, clearer navigation, responsive pages, and configurable animations.
- Per-instance Minecraft version and loader settings with separate game, mod, and world directories.
- Searchable Mojang catalog with every available release, snapshot, beta, and alpha version.
- Microsoft Device Code sign-in with Minecraft ownership checks and refresh-token storage in Windows Credential Manager.
- Fabric, Quilt, Forge, and NeoForge profile installation.
- Modrinth catalog that loads popular projects on open, with search, sorting, and SHA-1/SHA-512-verified installation.
- Isolated profiles, safe world ZIP import/export, mod management, PNG skin preview, Bedrock launch shortcut, and crash diagnostics.
- ECDSA-signed update manifest and SHA-256 installer verification. Unsigned or invalid updates are rejected.

## Build and test

Requirements: .NET 10 SDK, Windows 10/11 x64 for the release build, Python/Pillow for the application icon, and Inno Setup 6 to produce the installer.

```powershell
python -m pip install Pillow
python scripts/installer/build_icon.py
dotnet test minty-launcher.Tests/MintyLauncher.Tests.csproj --configuration Release
dotnet publish minty-launcher/MintyLauncher.csproj --configuration Release --runtime win-x64 --self-contained true --output dist/launcher
```

The signed GitHub release workflow uses the repository Actions secret `UPDATE_SIGNING_PRIVATE_KEY`. See [the build guide](./minty-launcher/docs/BUILDING.md) and [security policy](./minty-launcher/SECURITY.md) for signing details.

## Source layout

- [`minty-launcher/`](./minty-launcher/) — Avalonia UI, C# launcher services, and Java game runtime integration.
- [`minty-launcher.Tests/`](./minty-launcher.Tests/) — unit tests.
- [`.github/workflows/`](./.github/workflows/) — Windows CI and signed release pipeline.
