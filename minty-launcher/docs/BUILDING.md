# Building Minty Launcher 2.1.0

## Requirements

- .NET 10 SDK.
- Windows 10/11 x64 for Native AOT publishing and release verification.
- Python 3 and Pillow to generate the icon.
- Inno Setup 6 to compile the Windows installer.

Java is not needed to build the launcher. Minty uses a Java runtime only when launching Minecraft.

## Local checks

From the repository root:

```powershell
dotnet test minty-launcher.Tests/MintyLauncher.Tests.csproj --configuration Release
python -m pip install Pillow
python scripts/installer/build_icon.py
dotnet build minty-launcher/MintyLauncher.csproj --configuration Release
```

Native AOT publish on Windows x64:

```powershell
dotnet publish minty-launcher/MintyLauncher.csproj --configuration Release --runtime win-x64 --self-contained true --output dist/launcher
```

The publish is self-contained and does not require Go, Rust, or a separately installed .NET runtime.

## Signed release

The `v2.1.0` workflow tests the app, publishes a Windows x64 Native AOT executable, packages portable and installer builds, creates an ECDSA-signed update manifest, and publishes the release.

Before tagging, create a P-256 ECDSA private key and store its PEM contents as the repository Actions secret `UPDATE_SIGNING_PRIVATE_KEY`. Never commit or print the private key. The workflow derives the public key, embeds it in that release executable, and signs the SHA-256 manifest. The launcher rejects updates if its key is missing or the signature/hash is invalid.

The installer is compiled with `scripts/installer/MintySetup.iss`. Run `scripts/installer/build_icon.py` first to create `scripts/installer/Minty.ico`.
