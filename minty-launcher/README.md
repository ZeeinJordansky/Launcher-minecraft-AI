# Minty Launcher 2.2.0

Minty Launcher is a C# Avalonia desktop app for Windows x64. Java is used only to run Minecraft.

## Implemented

- Gray responsive interface with configurable accent color and simplified launch screen.
- Searchable full Mojang version catalog, including snapshots and historical beta/alpha releases.
- Per-instance Minecraft version and mod-loader settings with isolated folders.
- Microsoft Device Code sign-in, Minecraft ownership verification, and secure refresh-token storage through Windows Credential Manager.
- Fabric, Quilt, Forge, and NeoForge client profiles.
- Modrinth popular-content browser, search, sorting, and verified mod/resource-pack/shader installation.
- Isolated profiles, ZIP world import/export, mod removal, PNG skin preview, Bedrock launch shortcut, crash-report diagnostics, and launcher settings.
- Signed update manifest verification with ECDSA and SHA-256 installer verification.

## Build and test

```powershell
python ../scripts/installer/build_icon.py
dotnet test ../minty-launcher.Tests/MintyLauncher.Tests.csproj --configuration Release
dotnet publish MintyLauncher.csproj --configuration Release --runtime win-x64 --self-contained true --output ../dist/launcher
```

Release publishing requires the repository Actions secret `UPDATE_SIGNING_PRIVATE_KEY`. The workflow embeds its corresponding public key in the native executable and signs the update manifest. Without the secret, the workflow fails closed.

See [BUILDING.md](./docs/BUILDING.md), [ARCHITECTURE.md](./docs/ARCHITECTURE.md), and [SECURITY.md](./SECURITY.md).
