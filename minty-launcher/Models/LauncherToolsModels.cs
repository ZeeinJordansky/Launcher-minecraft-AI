namespace MintyLauncher.Models;

public sealed record WorldEntry(string Name, string Directory, DateTime LastModified);

public sealed record ModFileEntry(string Name, string Path, long Size, bool IsJar);

public sealed record LauncherDiagnostics(
    string OperatingSystem,
    string Architecture,
    int ProcessorCount,
    long TotalMemoryMb,
    string JavaPath,
    string? LatestCrashReport,
    string? LatestLogPath,
    IReadOnlyList<string> Warnings);
