# Minty Launcher project status

The 2.2.0 update refreshes the launcher UI and improves version, instance, and Modrinth catalog workflows. The app and launcher services are written in C#; Java is used only to run Minecraft.

## 2.2.0 changes

- Replaced the mint-tinted interface with a gray theme, selectable/custom HEX accent, readable navigation, and a launch-focused home page.
- Stored Minecraft version and mod-loader selection separately for each instance.
- Removed the 80-version display cap and added search and filters for releases, snapshots, beta, and alpha versions.
- Loads popular Modrinth projects when the catalog opens; supports search, sorting, and verified installation.
- Added regression tests for complete version results and browsing Modrinth without a search query or preselected loader.

## Existing limitations

- Skin Studio previews and saves imported PNG skins; it does not edit pixels or upload skins to Mojang.
- Modrinth `.mrpack` import is not implemented.
- Bedrock integration is a Windows launch shortcut and data-folder opener, not a full Bedrock profile manager.
- The diagnostics page provides basic environment and crash-log information, not a complete crash-analysis engine.
