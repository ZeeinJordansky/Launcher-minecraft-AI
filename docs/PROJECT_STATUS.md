# Minty Launcher project status

The active launcher in [`minty-launcher/`](../minty-launcher/) uses a C# / Avalonia interface and C# launcher services. Java remains only as the Minecraft runtime. The Rust and Go source trees have been removed from the active project.

The 2.2.0 update adds a gray, customizable interface; per-instance versions and loaders; the full Mojang version catalog; and automatic popular-project browsing in Modrinth.

The signed-release workflow requires the GitHub Actions secret `UPDATE_SIGNING_PRIVATE_KEY`. Publish the `v2.2.0` tag only after Windows CI and the signed release workflow succeed.
