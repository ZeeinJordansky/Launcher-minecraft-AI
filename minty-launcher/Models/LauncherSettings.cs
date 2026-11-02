using System.IO;

namespace MintyLauncher.Models;

public sealed class LauncherSettings
{
    public string GameDirectory { get; set; } = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".minecraft");
    public string SelectedVersion { get; set; } = "latest-release";
    public string Username { get; set; } = "Player";
    public string JavaPath { get; set; } = string.Empty;
    public int MaxMemoryMb { get; set; } = 4096;
    public bool WeakPcMode { get; set; }
    public bool EnableAnimations { get; set; } = true;
    public string ModLoader { get; set; } = "Vanilla";
    public bool IsolatedProfiles { get; set; }
}
