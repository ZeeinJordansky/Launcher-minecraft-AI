# Minty Launcher project status

The active 2.0.0-global application is [`minty-launcher-csharp/`](../minty-launcher-csharp/): a WPF frontend in C# and a Go engine, with Java used to run Minecraft. The legacy Rust launcher remains in [`minty-launcher/`](../minty-launcher/) but is no longer built or published by the 2.0.0-global workflows.

## Build and release

- CI runs Go tests and publishes a self-contained Windows x64 launcher package.
- The release workflow runs only for tag `v2.0.0-global`; it builds the installer and portable ZIP.
- A GitHub release is not published until that tag is pushed and the workflow succeeds.

## Known limitations

- Microsoft authentication and automated mod-loader installation/launch are not implemented.
- Modpack projects open on Modrinth for manual installation.
- Automatic Java installation currently supports Windows x64.
- The launcher is not Authenticode-signed.
