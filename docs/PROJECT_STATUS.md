# Minty Launcher project status

The active launcher is being migrated in [`minty-launcher/`](../minty-launcher/) to C# / Avalonia with Native AOT. Java remains only as the Minecraft runtime. The Rust and Go source trees have been removed from the active project.

The `v2.0.0-global` release remains the last published release. The `v2.1.0` implementation and release pipeline are not yet published; the Windows Native AOT job and signed release still need to complete.

The signed-release workflow intentionally requires the GitHub Actions secret `UPDATE_SIGNING_PRIVATE_KEY`. Do not create the `v2.1.0` tag until that secret is configured and the Windows CI publish is green.
