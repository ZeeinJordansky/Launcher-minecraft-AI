# Architecture

The desktop application is split into a WPF frontend and a standalone Go process:

1. The frontend serializes an action and settings as one JSON object and writes it to the engine's standard input.
2. The engine executes one request, emits JSON-line events to standard output, and exits with a non-zero status on errors.
3. The frontend reads progress and completion events while monitoring the process. Cancellation terminates the engine process tree.
4. The engine owns Mojang/Modrinth networking, settings persistence, Java runtime management, downloads, and game launch.

The release package places `minty-engine.exe` next to `minty-launcher.exe`. The WPF client prefers that packaged executable and uses `go run` only when launched from a development checkout with Go installed.

## Data directories

Settings are stored in the user's operating-system config directory under `Minty Launcher/settings.json`. Managed Java runtimes and launcher logs use the operating-system cache directory. Minecraft files use the selected game directory; isolated profiles use `instances/<version>` for the game working directory and mod content.

## Release

The `v2.0.0-global` tag triggers Windows x64 Go tests, engine cross-compilation, .NET self-contained publish, installer packaging, and release asset publication. Runtime content must remain alongside the WPF executable so the frontend can locate `minty-engine.exe`.
