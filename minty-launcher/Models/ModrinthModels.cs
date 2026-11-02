namespace MintyLauncher.Models;

public sealed record ModrinthProject(
    string Id,
    string ProjectType,
    string Slug,
    string Title,
    string Description,
    string? IconUrl,
    long Downloads,
    IReadOnlyList<string> Categories);

public sealed record MinecraftVersion(string Id, string Type, string ReleaseTime);
