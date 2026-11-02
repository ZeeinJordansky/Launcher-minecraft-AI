namespace MintyLauncher.Models;

public sealed record SecureUpdateResult(
    string Version,
    string InstallerPath,
    bool IsCurrent);
