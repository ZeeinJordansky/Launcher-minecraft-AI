using System.IO;
using System.Text.Json.Serialization;

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
    public bool CloseAfterGame { get; set; }
    public bool Fullscreen { get; set; }
    public bool AutoMemory { get; set; } = true;
    public int GraphicsProfile { get; set; }
    public string GarbageCollector { get; set; } = "G1GC";
    public string Language { get; set; } = "ru-RU";
    public string ModLoader { get; set; } = "Vanilla";
    public string ModLoaderVersion { get; set; } = string.Empty;
    public string SelectedLoaderProfileId { get; set; } = string.Empty;
    public bool IsolatedProfiles { get; set; }
    public string SelectedProfileId { get; set; } = "default";
    public string AccountId { get; set; } = string.Empty;
    public string AccountName { get; set; } = "Player";
    public string[] InstalledProfiles { get; set; } = ["default"];
}

[JsonSourceGenerationOptions(GenerationMode = JsonSourceGenerationMode.Metadata, WriteIndented = true)]
[JsonSerializable(typeof(LauncherSettings))]
internal partial class LauncherSettingsJsonContext : JsonSerializerContext;
