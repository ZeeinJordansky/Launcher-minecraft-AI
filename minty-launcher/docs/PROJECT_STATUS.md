# Minty Launcher project status

The 2.1.0 migration is underway in this folder: Avalonia UI and C# services replace the prior WPF/Go/Rust implementation. Java remains only as the Minecraft runtime. Unit tests cover isolated-profile paths, world archive import safety, mod deletion boundaries, and update-signature validation.

## Release status

`v2.1.0` has not been published by this change. The release workflow requires the maintainer to configure the Actions secret `UPDATE_SIGNING_PRIVATE_KEY`; it intentionally fails without a signing key. A Windows Native AOT publish and the signed release workflow still need to run successfully before claiming the release is available.

## Implemented surfaces

- Custom Avalonia navigation and themed home/settings pages.
- Mojang release/snapshot listing, game file installation and Java launch.
- Microsoft OAuth/device-code flow with Minecraft entitlement checks.
- Fabric/Quilt/Forge/NeoForge client profile installers.
- Modrinth mod/resource-pack/shader search and verified install, profile isolation, world ZIP import/export, and mod removal.
- PNG skin preview, Bedrock URI launch, system/crash diagnostics, and signed update download.

## Known gaps

- The Skin Studio currently previews/imports/exports PNG files; it is not a pixel editor and does not upload skins to Mojang.
- Modrinth modpack import is not implemented.
- Bedrock integration is a Windows launch shortcut and data-folder opener, not a full Bedrock profile manager.
- The UI does not yet include a hardware benchmark, broad crash-log heuristics, or installer auto-restart UX.
- Native AOT and release publishing must be validated on the Windows CI runner.
