using System.Security.Cryptography;
using System.Text.Json;
using MintyLauncher.Models;

namespace MintyLauncher.Services;

public sealed class ModrinthService(HttpClient? httpClient = null) : IDisposable
{
    private readonly HttpClient _httpClient = httpClient ?? CreateClient();
    private readonly bool _ownsClient = httpClient is null;

    public async Task<IReadOnlyList<ModrinthProject>> SearchAsync(
        string query,
        string category,
        string gameVersion,
        string loader,
        string projectType = "mod",
        string index = "relevance",
        CancellationToken cancellationToken = default)
    {
        if (projectType is not ("mod" or "resourcepack" or "shader"))
        {
            throw new ArgumentOutOfRangeException(nameof(projectType), "Этот тип контента пока не поддерживается.");
        }
        if (index is not ("relevance" or "downloads" or "follows" or "newest" or "updated"))
        {
            throw new ArgumentOutOfRangeException(nameof(index), "Неизвестный порядок сортировки Modrinth.");
        }
        var facets = new List<string[]> { new[] { $"project_type:{projectType}" } };
        if (!string.IsNullOrWhiteSpace(gameVersion))
        {
            facets.Add(new[] { $"versions:{gameVersion}" });
        }
        if (projectType == "mod" &&
            loader.ToLowerInvariant() is "fabric" or "forge" or "neoforge" or "quilt")
        {
            facets.Add(new[] { $"categories:{loader.ToLowerInvariant()}" });
        }
        if (!string.IsNullOrWhiteSpace(category) && category != "all")
        {
            facets.Add(new[] { $"categories:{category.ToLowerInvariant()}" });
        }

        var facetJson = JsonSerializer.Serialize(facets, ModrinthJsonContext.Default.ListStringArray);
        var uri = $"https://api.modrinth.com/v2/search?query={Uri.EscapeDataString(query)}" +
                  $"&facets={Uri.EscapeDataString(facetJson)}&index={index}&limit=50";
        using var response = await _httpClient.GetAsync(uri, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
            .ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        await using var stream = await response.Content.ReadAsStreamAsync(cancellationToken).ConfigureAwait(false);
        using var document = await JsonDocument.ParseAsync(stream, cancellationToken: cancellationToken)
            .ConfigureAwait(false);
        if (!document.RootElement.TryGetProperty("hits", out var hits) || hits.ValueKind != JsonValueKind.Array)
        {
            throw new InvalidDataException("Modrinth вернул некорректный список проектов.");
        }

        var projects = new List<ModrinthProject>();
        foreach (var hit in hits.EnumerateArray())
        {
            projects.Add(new ModrinthProject(
                GetString(hit, "project_id"),
                GetString(hit, "project_type"),
                GetString(hit, "slug"),
                GetString(hit, "title"),
                GetString(hit, "description"),
                GetNullableString(hit, "icon_url"),
                hit.TryGetProperty("downloads", out var downloads) ? downloads.GetInt64() : 0,
                hit.TryGetProperty("categories", out var categories) && categories.ValueKind == JsonValueKind.Array
                    ? categories.EnumerateArray().Select(static item => item.GetString() ?? string.Empty).ToArray()
                    : []));
        }

        return projects;
    }

    public async Task<string> InstallAsync(
        string projectId,
        string gameDirectory,
        string gameVersion,
        string loader,
        string projectType = "mod",
        CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(projectId) || projectId.Any(char.IsControl))
        {
            throw new ArgumentException("Указан неверный идентификатор проекта.", nameof(projectId));
        }

        if (projectType is not ("mod" or "resourcepack" or "shader"))
        {
            throw new ArgumentOutOfRangeException(nameof(projectType), "Этот тип контента пока не поддерживается.");
        }
        var versionUri = $"https://api.modrinth.com/v2/project/{Uri.EscapeDataString(projectId)}/version" +
                         $"?game_versions={Uri.EscapeDataString(JsonSerializer.Serialize(new[] { gameVersion }, ModrinthJsonContext.Default.StringArray))}";
        if (projectType == "mod")
        {
            if (loader.ToLowerInvariant() is not ("fabric" or "forge" or "neoforge" or "quilt"))
            {
                throw new InvalidOperationException("Для установки модов сначала выбери Fabric, Forge, NeoForge или Quilt.");
            }
            versionUri +=
                $"&loaders={Uri.EscapeDataString(JsonSerializer.Serialize(new[] { loader.ToLowerInvariant() }, ModrinthJsonContext.Default.StringArray))}";
        }
        using var versionsResponse = await _httpClient.GetAsync(
                versionUri, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
            .ConfigureAwait(false);
        versionsResponse.EnsureSuccessStatusCode();
        await using var versionsStream = await versionsResponse.Content.ReadAsStreamAsync(cancellationToken)
            .ConfigureAwait(false);
        using var versionsDocument = await JsonDocument.ParseAsync(
                versionsStream, cancellationToken: cancellationToken)
            .ConfigureAwait(false);
        if (versionsDocument.RootElement.ValueKind != JsonValueKind.Array ||
            versionsDocument.RootElement.GetArrayLength() == 0)
        {
            throw new InvalidOperationException("Для выбранной версии Minecraft нет совместимого файла.");
        }

        var firstVersion = versionsDocument.RootElement[0];
        if (!firstVersion.TryGetProperty("files", out var files) || files.GetArrayLength() == 0)
        {
            throw new InvalidDataException("Версия Modrinth не содержит файлов.");
        }
        var file = files.EnumerateArray().FirstOrDefault(static item =>
            item.TryGetProperty("primary", out var primary) && primary.GetBoolean());
        if (file.ValueKind == JsonValueKind.Undefined)
        {
            file = files[0];
        }

        var filename = GetString(file, "filename");
        if (Path.GetFileName(filename) != filename || filename.IndexOfAny(Path.GetInvalidFileNameChars()) >= 0)
        {
            throw new InvalidDataException("Modrinth вернул небезопасное имя файла.");
        }
        var url = GetString(file, "url");
        if (!Uri.TryCreate(url, UriKind.Absolute, out var downloadUri) ||
            downloadUri.Scheme != Uri.UriSchemeHttps)
        {
            throw new InvalidDataException("Modrinth вернул небезопасную ссылку на файл.");
        }

        var hashes = file.GetProperty("hashes");
        var expectedSha512 = GetNullableString(hashes, "sha512");
        var expectedSha1 = GetNullableString(hashes, "sha1");
        if (expectedSha512 is null && expectedSha1 is null)
        {
            throw new InvalidDataException("Файл Modrinth не содержит контрольной суммы.");
        }

        var contentDirectory = projectType switch
        {
            "mod" => "mods",
            "resourcepack" => "resourcepacks",
            "shader" => "shaderpacks",
            _ => throw new ArgumentOutOfRangeException(nameof(projectType))
        };
        var targetDirectory = Path.Combine(gameDirectory, contentDirectory);
        Directory.CreateDirectory(targetDirectory);
        var destination = Path.Combine(targetDirectory, filename);
        var temporaryPath = destination + "." + Guid.NewGuid().ToString("N") + ".download";
        try
        {
            using var response = await _httpClient.GetAsync(
                    downloadUri, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
                .ConfigureAwait(false);
            response.EnsureSuccessStatusCode();
            if (response.Content.Headers.ContentLength is > 1_073_741_824)
            {
                throw new InvalidDataException("Размер файла Modrinth превышает допустимый предел.");
            }

            await using (var input = await response.Content.ReadAsStreamAsync(cancellationToken).ConfigureAwait(false))
            await using (var output = new FileStream(
                             temporaryPath, FileMode.CreateNew, FileAccess.Write, FileShare.None, 81920,
                             FileOptions.Asynchronous | FileOptions.SequentialScan))
            {
                using var sha512 = expectedSha512 is not null ? IncrementalHash.CreateHash(HashAlgorithmName.SHA512) : null;
                using var sha1 = expectedSha1 is not null ? IncrementalHash.CreateHash(HashAlgorithmName.SHA1) : null;
                var buffer = new byte[81920];
                long totalBytes = 0;
                int count;
                while ((count = await input.ReadAsync(buffer, cancellationToken).ConfigureAwait(false)) != 0)
                {
                    totalBytes = checked(totalBytes + count);
                    if (totalBytes > 1_073_741_824)
                    {
                        throw new InvalidDataException("Размер файла Modrinth превышает допустимый предел.");
                    }
                    sha512?.AppendData(buffer, 0, count);
                    sha1?.AppendData(buffer, 0, count);
                    await output.WriteAsync(buffer.AsMemory(0, count), cancellationToken).ConfigureAwait(false);
                }

                var sha512Matches = expectedSha512 is null ||
                                   Convert.ToHexString(sha512!.GetHashAndReset()).Equals(
                                       expectedSha512, StringComparison.OrdinalIgnoreCase);
                var sha1Matches = expectedSha1 is null ||
                                 Convert.ToHexString(sha1!.GetHashAndReset()).Equals(
                                     expectedSha1, StringComparison.OrdinalIgnoreCase);
                if (!sha512Matches || !sha1Matches)
                {
                    throw new InvalidDataException("Контрольная сумма файла Modrinth не совпала.");
                }
            }

            File.Move(temporaryPath, destination, overwrite: true);
            return destination;
        }
        finally
        {
            if (File.Exists(temporaryPath))
            {
                File.Delete(temporaryPath);
            }
        }
    }

    public void Dispose()
    {
        if (_ownsClient)
        {
            _httpClient.Dispose();
        }
    }

    private static HttpClient CreateClient()
    {
        var client = new HttpClient { Timeout = TimeSpan.FromMinutes(5) };
        client.DefaultRequestHeaders.UserAgent.ParseAdd("MintyLauncher/2.2.0 (https://github.com/ZeeinJordansky/Launcher-minecraft-AI)");
        return client;
    }

    private static string GetString(JsonElement element, string property) =>
        GetNullableString(element, property) ?? throw new InvalidDataException($"В ответе Modrinth отсутствует {property}.");

    private static string? GetNullableString(JsonElement element, string property) =>
        element.TryGetProperty(property, out var value) && value.ValueKind == JsonValueKind.String
            ? value.GetString()
            : null;
}
