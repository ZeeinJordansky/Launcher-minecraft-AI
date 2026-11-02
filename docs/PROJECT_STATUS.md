# Minty Launcher project status

The active 2.0.0-global application lives directly in [`minty-launcher/`](../minty-launcher/): a WPF frontend in C# and a Go engine, with Java used to run Minecraft. Legacy Rust source files remain in that folder but are no longer built or published by the 2.0.0-global workflows.

## Build and release

- CI runs Go tests and publishes a self-contained Windows x64 launcher package.
- The release workflow runs only for tag `v2.0.0-global`; it builds the installer and portable ZIP.
- A GitHub release is not published until that tag is pushed and the workflow succeeds.

## Known limitations

- Microsoft authentication and automated mod-loader installation/launch are not implemented.
- Modpack projects open on Modrinth for manual installation.
- Automatic Java installation currently supports Windows x64.
- The launcher is not Authenticode-signed.
