# Minty Launcher 2.0.0-global

The active launcher source is in this folder: a C# / WPF desktop frontend, a Go backend, and Java to run Minecraft. The legacy Rust sources remain here but are no longer part of the 2.0.0-global build.

## Architecture

- `MainWindow.xaml` and `MainWindow.xaml.cs`: custom Minty UI, navigation, settings, versions, Modrinth catalog, and progress.
- `Services/EngineClient.cs`: starts `minty-engine.exe` and exchanges JSON-lines requests/events over standard input/output.
- `engine/cmd/minty-engine`: Go command dispatcher.
- `engine/internal/minecraft`: Mojang manifest, verified downloads, runtime setup, and Java launch.
- `engine/internal/mods`: Modrinth search and verified compatible file installation.
- `engine/internal/settings`: validated, atomic user settings and profile paths.

The Go executable is shipped next to the frontend. During development, the UI can start the engine with `go run` when the Go source tree is available.

## Supported behavior

- Search and install Vanilla Minecraft Java versions.
- Install Windows Temurin runtimes automatically when a compatible runtime is not configured.
- Search Modrinth for mods, modpacks, resource packs, and shaders with category and sort filters.
- Install compatible mods, resource packs, and shaders into shared or per-version profile folders.
- Configure an offline username, Java executable, game path, memory limit, low-spec JVM flags, and interface animations.
- Cancel active downloads and open the current mods folder.

Microsoft authentication and automated mod-loader installation/launch are not implemented. Modloader selection is used as a Modrinth compatibility filter; launching a non-Vanilla loader is explicitly rejected instead of silently starting the wrong profile. Modpacks open their Modrinth page.

## Local checks

```sh
cd engine
go test ./...
```

On Windows with .NET 10:

```powershell
dotnet build MintyLauncher.csproj
```

The repository CI builds a self-contained Windows x64 publish directory and the release workflow packages it with `minty-engine.exe`.
