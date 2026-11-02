# Minty Launcher

Minty Launcher 2.0.0-global is a Windows x64 Minecraft Java launcher with a custom WPF interface, a Go engine, and Java for running the game.

## Download

The `v2.0.0-global` release publishes a Windows installer and a portable ZIP. The portable ZIP contains the launcher, its Go engine, and the .NET runtime files it needs. It does not require Go or a separate .NET installation.

## Features

- Original Minty dark interface with a custom profile avatar, animated page transitions, and resizable window.
- Mojang version catalog with search and release/snapshot filters.
- Minecraft installation and launch, including verified game assets and managed Temurin Java runtimes on Windows.
- Modrinth search for mods, modpacks, resource packs, and shaders; popularity and recency sorting.
- Installs compatible mods, resource packs, and shaders into shared or version-isolated folders. Modpacks open on Modrinth.
- Offline profile, Java path selection, memory limit, low-spec JVM options, and version-isolated game data.
- Cancellable downloads and an option to open the active mods directory.

## Current limitations

- Minecraft authentication is offline-only; Microsoft sign-in is not implemented.
- This release launches Vanilla Minecraft. Fabric, Forge, NeoForge, and Quilt can be selected as Modrinth compatibility filters, but the launcher does not install or launch those loaders.
- Modpack projects open on Modrinth instead of being imported automatically.
- Automatic Java installation currently supports Windows x64.
- The launcher is not Authenticode-signed.

## Source layout

- [`minty-launcher/`](./minty-launcher/) — active C# WPF frontend and Go engine. Legacy Rust source files remain in the same folder but are not used by the 2.0.0-global workflows.
- [`.github/workflows/`](./.github/workflows/) — Windows CI and tag-triggered release pipeline.

## Build and test

Run Go tests:

```sh
cd minty-launcher/engine
go test ./...
```

Build the WPF frontend on Windows with .NET 10:

```powershell
dotnet build minty-launcher/MintyLauncher.csproj
```

The release workflow cross-compiles the Go engine for Windows x64, publishes a self-contained .NET application, packages the portable ZIP and installer, and runs the Go test suite.
