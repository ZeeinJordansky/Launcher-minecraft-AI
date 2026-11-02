# Building Minty Launcher 2.0.0-global

## Requirements

- Windows 10/11 x64 for running the WPF interface.
- .NET 10 SDK for building the C# frontend.
- Go 1.23 or later for tests and the Go engine.
- Pillow for generating the application and installer icon.
- Inno Setup 6 to build the Windows installer.

Minecraft Java Runtime is downloaded by the launcher as needed; a JDK is not needed to build or run the launcher.

## Local checks

From the repository root, run the Go tests:

```sh
cd minty-launcher/engine
go test ./...
go vet ./...
```

Build the WPF frontend from the repository root on Windows:

```powershell
dotnet build minty-launcher/MintyLauncher.csproj
```

To package the Windows application, first build `minty-engine.exe` for Windows x64 into `minty-launcher/engine/dist/`, generate the icon with `python scripts/installer/build_icon.py`, then publish:

```powershell
dotnet publish minty-launcher/MintyLauncher.csproj --configuration Release --runtime win-x64 --self-contained true --output dist/launcher
```

The published directory includes the Go engine beside the launcher. The root GitHub Actions workflows perform this packaging, validate both PE files, build a portable ZIP, and create the installer for tag `v2.0.0-global`.

The legacy Rust files remain in this folder for reference; they are not used by the current CI or release workflow.
