using System.Text.Json.Serialization;

namespace MintyLauncher.Models;

[JsonSourceGenerationOptions(GenerationMode = JsonSourceGenerationMode.Metadata)]
[JsonSerializable(typeof(List<string[]>))]
[JsonSerializable(typeof(string[]))]
internal partial class ModrinthJsonContext : JsonSerializerContext;
