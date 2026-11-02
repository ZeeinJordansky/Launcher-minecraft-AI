# Architecture

Minty Launcher is a single C# Avalonia application:

1. `MainWindow` owns navigation and invokes typed launcher services.
2. `MinecraftLauncherService` reads Mojang metadata, validates/downloads client libraries and assets, installs supported loader profiles, and starts Java with argument-list APIs.
3. `ModrinthService` searches the Modrinth API and checks published file hashes before installing mods.
4. `MinecraftAuthenticationService` performs Microsoft Device Code → Xbox Live → XSTS → Minecraft Services authentication, checks entitlements, and stores only the refresh token in Windows Credential Manager.
5. `LauncherToolsService` manages isolated profiles and world archives and gathers crash/system diagnostics.
6. `SecureUpdateService` verifies an ECDSA signature over the exact release-manifest bytes before downloading the installer, then checks the signed SHA-256 digest.

Network JSON that is mapped to C# models uses `System.Text.Json` source generation. Dynamic Mojang/loader metadata is inspected with `JsonDocument`; loader profile merging preserves the vanilla metadata and appends loader libraries/arguments.

## Data locations

- Launcher settings: `%APPDATA%/MintyLauncher/settings.json`.
- Managed Java runtimes and signed updates: `%APPDATA%/MintyLauncher/`.
- Minecraft versions, libraries, assets, worlds, and mods: configured game directory.
- Isolated profile data: `<game directory>/profiles/<profile id>`.
- Microsoft refresh token: Windows Credential Manager. Minecraft access tokens remain in process memory.

## Native AOT and signed updates

Release builds use .NET Native AOT and trimming. The release workflow requires `UPDATE_SIGNING_PRIVATE_KEY`, derives the matching public key into the release binary, and signs the exact update manifest bytes with ECDSA/SHA-256. The private key is never written to the repository or release artifact. Update verification has no unsigned fallback.
