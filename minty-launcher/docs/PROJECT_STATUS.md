# Minty Launcher project status

The active 2.0.0-global app is built directly in this folder: a C# WPF frontend and Go engine, using Java to run Minecraft. The legacy Rust sources remain here but are not used by current CI or release workflows.

## Release

The root `release.yml` workflow runs when tag `v2.0.0-global` is pushed. It tests and builds the Go engine, publishes a self-contained Windows x64 WPF app, packages an installer and portable ZIP, and creates the GitHub release.

The tag currently needs to reference the commit containing the relocated app sources and current workflow. If a prior tag exists, moving it requires a force push.

## Current limitations

- Microsoft account authentication is not implemented; profiles are offline-only.
- Fabric, Forge, NeoForge, and Quilt can filter Modrinth search results but are not installed or launched automatically.
- Modpack projects open on Modrinth rather than importing `.mrpack` dependencies.
- Automatic Java installation currently supports Windows x64.
- The launcher is not Authenticode-signed.
