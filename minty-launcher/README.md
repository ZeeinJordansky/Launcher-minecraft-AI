# Minty Launcher 2.1.0

Minty Launcher is an Avalonia desktop app written in C# and published with Native AOT for Windows x64. Java is used only to run Minecraft.

## Implemented

- Mojang release/snapshot lookup and vanilla game download/launch with hash verification.
- Microsoft Device Code sign-in, Minecraft ownership verification, and secure refresh-token storage through Windows Credential Manager.
- Fabric, Quilt, Forge, and NeoForge client profiles.
- Modrinth mod, resource-pack, and shader search with verified installation.
- Isolated profiles, ZIP world import/export, mod removal, PNG skin preview, Bedrock launch shortcut, crash-report diagnostics, and launcher settings.
- Signed update manifest verification with ECDSA and SHA-256 installer verification.

## Build and test

```powershell
dotnet test ../minty-launcher.Tests/MintyLauncher.Tests.csproj --configuration Release
python ../scripts/installer/build_icon.py
dotnet publish MintyLauncher.csproj --configuration Release --runtime win-x64 --self-contained true --output ../dist/launcher
```

Release publishing requires the repository Actions secret `UPDATE_SIGNING_PRIVATE_KEY`. The workflow embeds its corresponding public key in the native executable and signs the update manifest. Without the secret, the workflow fails closed.

See [BUILDING.md](./docs/BUILDING.md), [ARCHITECTURE.md](./docs/ARCHITECTURE.md), and [SECURITY.md](./SECURITY.md).
