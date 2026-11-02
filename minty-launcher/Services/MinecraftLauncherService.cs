using System.Diagnostics;
using System.IO.Compression;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.RegularExpressions;
using System.Xml;
using System.Xml.Linq;
using MintyLauncher.Models;

namespace MintyLauncher.Services;

public sealed class MinecraftLauncherService(HttpClient? httpClient = null) : IDisposable
{
    private const string VersionManifestUrl = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
    private readonly HttpClient _httpClient = httpClient ?? CreateClient();
    private readonly bool _ownsClient = httpClient is null;

    public async Task<IReadOnlyList<MinecraftVersion>> GetVersionsAsync(
        CancellationToken cancellationToken = default)
    {
        using var response = await _httpClient.GetAsync(
                VersionManifestUrl, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
            .ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        var manifestBytes = await ReadContentBoundedAsync(response.Content, 16_000_000, cancellationToken)
            .ConfigureAwait(false);
        using var document = JsonDocument.Parse(manifestBytes);
        if (!document.RootElement.TryGetProperty("versions", out var versions) ||
            versions.ValueKind != JsonValueKind.Array)
        {
            throw new InvalidDataException("Официальный каталог Minecraft вернул неверный формат.");
        }

        return versions.EnumerateArray().Select(static item => new MinecraftVersion(
            RequiredString(item, "id"),
            RequiredString(item, "type"),
            RequiredString(item, "releaseTime"))).ToArray();
    }

    public async Task InstallAndLaunchAsync(
        LauncherSettings settings,
        MinecraftAccount? account,
        IProgress<(string Message, int? Percent)>? progress = null,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(settings);
        var manifest = await GetManifestAsync(cancellationToken).ConfigureAwait(false);
        var version = settings.SelectedVersion;
        if (version == "latest-release")
        {
            version = manifest.LatestRelease;
        }
        else if (version == "latest-snapshot")
        {
            version = manifest.LatestSnapshot;
        }
        else if (!IsSafePathSegment(version))
        {
            throw new ArgumentException("Указана неверная версия Minecraft.", nameof(settings));
        }

        var gameDirectory = Path.GetFullPath(settings.GameDirectory);
        Directory.CreateDirectory(gameDirectory);
        var versionDirectory = Path.Combine(gameDirectory, "versions", version);
        Directory.CreateDirectory(versionDirectory);
        progress?.Report(($"Подготовка Minecraft {version}", 2));
        var manifestEntry = manifest.Versions
            .FirstOrDefault(candidate => candidate.Id == version)
            ?? throw new InvalidOperationException($"Версия Minecraft {version} отсутствует в официальном каталоге.");
        using var baseMetadata = await DownloadJsonAsync(manifestEntry.Url, cancellationToken).ConfigureAwait(false);
        DownloadedJson? mergedMetadata = null;
        using var metadata = ResolveLoaderProfileIfSelected(
            baseMetadata,
            settings,
            version,
            gameDirectory,
            out mergedMetadata);
        var launchVersionId = settings.ModLoader == "Vanilla"
            ? version
            : settings.SelectedLoaderProfileId;

        var client = metadata.RootElement.GetProperty("downloads").GetProperty("client");
        var clientPath = Path.Combine(versionDirectory, version + ".jar");
        await DownloadVerifiedAsync(
                RequiredString(client, "url"),
                clientPath,
                GetNullableString(client, "sha1"),
                cancellationToken)
            .ConfigureAwait(false);
        progress?.Report(("Клиентская часть проверена", 10));

        var librariesDirectory = Path.Combine(gameDirectory, "libraries");
        var nativesDirectory = Path.Combine(versionDirectory, "natives");
        Directory.CreateDirectory(nativesDirectory);
        var libraries = metadata.RootElement.TryGetProperty("libraries", out var libraryList)
            ? libraryList.EnumerateArray().ToArray()
            : [];
        var classpath = new List<string> { clientPath };
        foreach (var library in libraries)
        {
            cancellationToken.ThrowIfCancellationRequested();
            if (!IsAllowedByRules(
                    library, "windows", Environment.Is64BitOperatingSystem ? "x86_64" : "x86"))
            {
                continue;
            }

            var downloads = library.TryGetProperty("downloads", out var downloadObject)
                ? downloadObject
                : default;
            if (downloads.ValueKind == JsonValueKind.Object &&
                downloads.TryGetProperty("artifact", out var artifact))
            {
                var artifactPath = ResolveLibraryPath(librariesDirectory, RequiredString(artifact, "path"));
                await DownloadVerifiedAsync(
                        RequiredString(artifact, "url"),
                        artifactPath,
                        GetNullableString(artifact, "sha1"),
                        cancellationToken)
                    .ConfigureAwait(false);
                classpath.Add(artifactPath);
            }
            else if (library.TryGetProperty("name", out var mavenName) &&
                     mavenName.ValueKind == JsonValueKind.String)
            {
                var relativePath = ResolveMavenArtifact(mavenName.GetString()!);
                var artifactPath = ResolveLibraryPath(librariesDirectory, relativePath);
                var baseUrl = GetNullableString(library, "url") ?? "https://libraries.minecraft.net/";
                if (!baseUrl.EndsWith('/'))
                {
                    baseUrl += "/";
                }
                var artifactUrl = new Uri(new Uri(baseUrl, UriKind.Absolute), relativePath.Replace('\\', '/')).ToString();
                await DownloadVerifiedAsync(artifactUrl, artifactPath, null, cancellationToken)
                    .ConfigureAwait(false);
                classpath.Add(artifactPath);
            }

            if (downloads.ValueKind == JsonValueKind.Object &&
                downloads.TryGetProperty("classifiers", out var classifiers) &&
                library.TryGetProperty("natives", out var natives) &&
                natives.TryGetProperty("windows", out var windowsNative))
            {
                var classifierName = windowsNative.GetString()?.Replace(
                    "${arch}", Environment.Is64BitOperatingSystem ? "64" : "32", StringComparison.Ordinal);
                if (!string.IsNullOrWhiteSpace(classifierName) &&
                    classifiers.TryGetProperty(classifierName, out var classifier))
                {
                    var nativePath = ResolveLibraryPath(librariesDirectory, RequiredString(classifier, "path"));
                    await DownloadVerifiedAsync(
                            RequiredString(classifier, "url"),
                            nativePath,
                            GetNullableString(classifier, "sha1"),
                            cancellationToken)
                        .ConfigureAwait(false);
                    ExtractNativeJar(nativePath, nativesDirectory);
                }
            }
        }
        progress?.Report(("Библиотеки игры готовы", 68));

        var assetIndex = metadata.RootElement.GetProperty("assetIndex");
        var assetIndexId = RequiredString(assetIndex, "id");
        var assetIndexPath = Path.Combine(gameDirectory, "assets", "indexes", assetIndexId + ".json");
        await DownloadVerifiedAsync(
                RequiredString(assetIndex, "url"),
                assetIndexPath,
                GetNullableString(assetIndex, "sha1"),
                cancellationToken)
            .ConfigureAwait(false);
        await InstallAssetsAsync(gameDirectory, assetIndexPath, progress, cancellationToken).ConfigureAwait(false);
        progress?.Report(("Игровые ресурсы готовы", 92));

        var versionJsonPath = Path.Combine(versionDirectory, version + ".json");
        await File.WriteAllBytesAsync(versionJsonPath, baseMetadata.Raw, cancellationToken).ConfigureAwait(false);
        var javaPath = await ResolveJavaAsync(settings.JavaPath, version, cancellationToken).ConfigureAwait(false);
        var startInfo = CreateGameStartInfo(
            metadata.RootElement,
            settings,
            account,
            launchVersionId,
            gameDirectory,
            assetIndexId,
            nativesDirectory,
            classpath,
            javaPath);
        progress?.Report(("Запуск Minecraft", 100));
        var process = Process.Start(startInfo)
            ?? throw new InvalidOperationException("Не удалось запустить Java.");
        if (settings.CloseAfterGame)
        {
            process.Dispose();
        }
    }

    private static DownloadedJson ResolveLoaderProfileIfSelected(
            DownloadedJson baseMetadata,
            LauncherSettings settings,
            string gameVersion,
            string gameDirectory,
            out DownloadedJson? mergedMetadata)
        {
            mergedMetadata = null;
            if (settings.ModLoader == "Vanilla")
            {
                return baseMetadata;
            }
            var profileId = settings.SelectedLoaderProfileId;
            if (!IsSafePathSegment(profileId))
            {
                throw new InvalidOperationException($"Сначала установи загрузчик {settings.ModLoader} для Minecraft {gameVersion}.");
            }
            var profilePath = Path.Combine(gameDirectory, "versions", profileId, profileId + ".json");
            if (!File.Exists(profilePath))
            {
                throw new FileNotFoundException($"Профиль {settings.ModLoader} отсутствует. Установи загрузчик в разделе «Сборки».");
            }
            using var profile = JsonDocument.Parse(File.ReadAllBytes(profilePath));
            if (GetNullableString(profile.RootElement, "inheritsFrom") != gameVersion)
            {
                throw new InvalidOperationException($"Профиль {settings.ModLoader} несовместим с Minecraft {gameVersion}.");
            }
            mergedMetadata = MergeVersionMetadata(baseMetadata, profile.RootElement, profileId);
            return mergedMetadata;
        }

    private static DownloadedJson MergeVersionMetadata(
            DownloadedJson baseMetadata,
            JsonElement profile,
            string profileId)
        {
            var baseObject = JsonNode.Parse(baseMetadata.Raw)?.AsObject()
                ?? throw new InvalidDataException("Профиль Minecraft пуст.");
            var profileObject = JsonNode.Parse(profile.GetRawText())?.AsObject()
                ?? throw new InvalidDataException("Профиль загрузчика пуст.");
            foreach (var property in profileObject)
            {
                if (property.Key is "id" or "inheritsFrom" or "time" or "releaseTime" or "type")
                {
                    continue;
                }
                if (property.Key == "libraries" &&
                    property.Value is JsonArray profileLibraries &&
                    baseObject[property.Key] is JsonArray baseLibraries)
                {
                    var mergedLibraries = new JsonArray();
                    foreach (var library in baseLibraries)
                    {
                        mergedLibraries.Add(library?.DeepClone());
                    }
                    foreach (var library in profileLibraries)
                    {
                        mergedLibraries.Add(library?.DeepClone());
                    }
                    baseObject[property.Key] = mergedLibraries;
                }
                else if (property.Key == "arguments" &&
                         property.Value is JsonObject profileArguments &&
                         baseObject[property.Key] is JsonObject baseArguments)
                {
                    foreach (var argumentGroup in profileArguments)
                    {
                        if (argumentGroup.Value is JsonArray appendedArguments &&
                            baseArguments[argumentGroup.Key] is JsonArray existingArguments)
                        {
                            foreach (var argument in appendedArguments)
                            {
                                existingArguments.Add(argument?.DeepClone());
                            }
                        }
                        else
                        {
                            baseArguments[argumentGroup.Key] = argumentGroup.Value?.DeepClone();
                        }
                    }
                }
                else
                {
                    baseObject[property.Key] = property.Value?.DeepClone();
                }
            }
            baseObject["id"] = profileId;
            var bytes = Encoding.UTF8.GetBytes(baseObject.ToJsonString());
            return new DownloadedJson(JsonDocument.Parse(bytes), bytes);
        }

    private static IComparer<int[]> NumericVersionComparer { get; } = Comparer<int[]>.Create(static (left, right) =>
    {
        for (var index = 0; index < Math.Min(left.Length, right.Length); index++)
        {
            var comparison = left[index].CompareTo(right[index]);
            if (comparison != 0)
            {
                return comparison;
            }
        }
        return left.Length.CompareTo(right.Length);
    });

    public async Task<string> InstallFabricAsync(
        string gameDirectory,
        string gameVersion,
        string loaderVersion = "",
        CancellationToken cancellationToken = default)
    {
        if (!IsSafePathSegment(gameVersion))
        {
            throw new ArgumentException("Указана неверная версия Minecraft.", nameof(gameVersion));
        }
        using var versions = await DownloadJsonAsync(
            $"https://meta.fabricmc.net/v2/versions/loader/{Uri.EscapeDataString(gameVersion)}",
            cancellationToken).ConfigureAwait(false);
        var entry = versions.RootElement.EnumerateArray().FirstOrDefault(item =>
            string.IsNullOrWhiteSpace(loaderVersion) ||
            item.GetProperty("loader").GetProperty("version").GetString() == loaderVersion);
        if (entry.ValueKind == JsonValueKind.Undefined)
        {
            throw new InvalidOperationException($"Fabric не поддерживает Minecraft {gameVersion} с этой версией загрузчика.");
        }

        var selectedLoader = entry.GetProperty("loader").GetProperty("version").GetString()
            ?? throw new InvalidDataException("Fabric вернул пустую версию загрузчика.");
        using var profile = await DownloadJsonAsync(
            $"https://meta.fabricmc.net/v2/versions/loader/{Uri.EscapeDataString(gameVersion)}/{Uri.EscapeDataString(selectedLoader)}/profile/json",
            cancellationToken).ConfigureAwait(false);
        return await SaveLoaderProfileAsync(gameDirectory, gameVersion, profile, "Fabric", cancellationToken)
            .ConfigureAwait(false);
    }

    public async Task<string> InstallQuiltAsync(
        string gameDirectory,
        string gameVersion,
        string loaderVersion = "",
        CancellationToken cancellationToken = default)
    {
        if (!IsSafePathSegment(gameVersion))
        {
            throw new ArgumentException("Указана неверная версия Minecraft.", nameof(gameVersion));
        }
        using var versions = await DownloadJsonAsync(
            $"https://meta.quiltmc.org/v3/versions/loader/{Uri.EscapeDataString(gameVersion)}",
            cancellationToken).ConfigureAwait(false);
        var entry = versions.RootElement.EnumerateArray().FirstOrDefault(item =>
            string.IsNullOrWhiteSpace(loaderVersion) ||
            item.GetProperty("loader").GetProperty("version").GetString() == loaderVersion);
        if (entry.ValueKind == JsonValueKind.Undefined)
        {
            throw new InvalidOperationException($"Quilt не поддерживает Minecraft {gameVersion} с этой версией загрузчика.");
        }

        var selectedLoader = entry.GetProperty("loader").GetProperty("version").GetString()
            ?? throw new InvalidDataException("Quilt вернул пустую версию загрузчика.");
        using var profile = await DownloadJsonAsync(
            $"https://meta.quiltmc.org/v3/versions/loader/{Uri.EscapeDataString(gameVersion)}/{Uri.EscapeDataString(selectedLoader)}/profile/json",
            cancellationToken).ConfigureAwait(false);
        return await SaveLoaderProfileAsync(gameDirectory, gameVersion, profile, "Quilt", cancellationToken)
            .ConfigureAwait(false);
    }

    public async Task<string> InstallForgeAsync(
        string gameDirectory,
        string gameVersion,
        string loaderVersion = "",
        CancellationToken cancellationToken = default)
    {
        if (!IsSafePathSegment(gameVersion))
        {
            throw new ArgumentException("Указана неверная версия Minecraft.", nameof(gameVersion));
        }
        var forgeVersion = loaderVersion;
        if (string.IsNullOrWhiteSpace(forgeVersion))
        {
            using var promotions = await DownloadJsonAsync(
                "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json",
                cancellationToken).ConfigureAwait(false);
            var promos = promotions.RootElement.GetProperty("promos");
            var recommendedId = gameVersion + "-recommended";
            var latestId = gameVersion + "-latest";
            if (promos.TryGetProperty(recommendedId, out var recommended))
            {
                forgeVersion = recommended.GetString() ?? string.Empty;
            }
            else if (promos.TryGetProperty(latestId, out var latest))
            {
                forgeVersion = latest.GetString() ?? string.Empty;
            }
            else
            {
                throw new InvalidOperationException($"Forge не публикует установщик для Minecraft {gameVersion}.");
            }
        }
        if (!IsSafeSegmentForVersion(forgeVersion))
        {
            throw new InvalidDataException("Forge вернул неверную версию загрузчика.");
        }
        var combined = $"{gameVersion}-{forgeVersion}";
        var installerUri =
            $"https://maven.minecraftforge.net/net/minecraftforge/forge/{Uri.EscapeDataString(combined)}/forge-{Uri.EscapeDataString(combined)}-installer.jar";
        return await InstallExternalLoaderAsync(gameDirectory, gameVersion, "Forge", forgeVersion, installerUri,
            cancellationToken).ConfigureAwait(false);
    }

    public async Task<string> InstallNeoForgeAsync(
        string gameDirectory,
        string gameVersion,
        string loaderVersion = "",
        CancellationToken cancellationToken = default)
    {
        if (!IsSafePathSegment(gameVersion))
        {
            throw new ArgumentException("Указана неверная версия Minecraft.", nameof(gameVersion));
        }
        var parts = gameVersion.Split('.');
        if (parts.Length < 2 || parts[0] != "1" || !int.TryParse(parts[1], out var minor))
        {
            throw new InvalidOperationException($"Не удалось сопоставить Minecraft {gameVersion} с NeoForge.");
        }
        var patch = parts.Length > 2 && int.TryParse(parts[2], out var parsedPatch) ? parsedPatch : 0;
        var prefix = $"{minor}.{patch}.";
        var neoForgeVersion = loaderVersion;
        if (string.IsNullOrWhiteSpace(neoForgeVersion))
        {
            using var response = await _httpClient.GetAsync(
                    "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml",
                    HttpCompletionOption.ResponseHeadersRead,
                    cancellationToken)
                .ConfigureAwait(false);
            response.EnsureSuccessStatusCode();
            await using var xmlStream = await response.Content.ReadAsStreamAsync(cancellationToken).ConfigureAwait(false);
            using var reader = XmlReader.Create(xmlStream, new XmlReaderSettings
            {
                DtdProcessing = DtdProcessing.Prohibit,
                XmlResolver = null,
                MaxCharactersInDocument = 2_000_000
            });
            var document = await XDocument.LoadAsync(reader, LoadOptions.None, cancellationToken).ConfigureAwait(false);
            neoForgeVersion = document.Descendants("version")
                .Select(static element => element.Value)
                .Where(version => version.StartsWith(prefix, StringComparison.Ordinal) && !version.Contains("beta", StringComparison.OrdinalIgnoreCase))
                .OrderBy(version => version.Split('.').Select(part => int.TryParse(part, out var number) ? number : 0).ToArray(),
                    NumericVersionComparer)
                .LastOrDefault()
                ?? throw new InvalidOperationException($"NeoForge не публикует установщик для Minecraft {gameVersion}.");
        }
        if (!IsSafeSegmentForVersion(neoForgeVersion))
        {
            throw new InvalidDataException("NeoForge вернул неверную версию загрузчика.");
        }
        var installerUri =
            $"https://maven.neoforged.net/releases/net/neoforged/neoforge/{Uri.EscapeDataString(neoForgeVersion)}/neoforge-{Uri.EscapeDataString(neoForgeVersion)}-installer.jar";
        return await InstallExternalLoaderAsync(gameDirectory, gameVersion, "NeoForge", neoForgeVersion, installerUri,
            cancellationToken).ConfigureAwait(false);
    }

    private async Task<string> InstallExternalLoaderAsync(
        string gameDirectory,
        string gameVersion,
        string loader,
        string loaderVersion,
        string installerUri,
        CancellationToken cancellationToken)
    {
        var installersDirectory = Path.Combine(gameDirectory, ".minty", "installers");
        Directory.CreateDirectory(installersDirectory);
        var installerPath = Path.Combine(installersDirectory, $"{loader.ToLowerInvariant()}-{loaderVersion}-installer.jar");
        await DownloadVerifiedAsync(installerUri, installerPath, null, cancellationToken).ConfigureAwait(false);
        var javaPath = await ResolveJavaAsync(string.Empty, gameVersion, cancellationToken).ConfigureAwait(false);
        var startInfo = new ProcessStartInfo(javaPath)
        {
            WorkingDirectory = gameDirectory,
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true
        };
        startInfo.ArgumentList.Add("-jar");
        startInfo.ArgumentList.Add(installerPath);
        startInfo.ArgumentList.Add("--installClient");
        startInfo.ArgumentList.Add(gameDirectory);
        using var process = Process.Start(startInfo)
            ?? throw new InvalidOperationException($"Не удалось запустить установщик {loader}.");
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(TimeSpan.FromMinutes(10));
        var stdoutTask = process.StandardOutput.ReadToEndAsync();
        var stderrTask = process.StandardError.ReadToEndAsync();
        try
        {
            await process.WaitForExitAsync(timeout.Token).ConfigureAwait(false);
        }
        catch (OperationCanceledException)
        {
            if (!process.HasExited)
            {
                process.Kill(entireProcessTree: true);
                await process.WaitForExitAsync(CancellationToken.None).ConfigureAwait(false);
            }
            await Task.WhenAll(stdoutTask, stderrTask).ConfigureAwait(false);
            throw;
        }
        var output = await stdoutTask.ConfigureAwait(false);
        var errorOutput = await stderrTask.ConfigureAwait(false);
        if (process.ExitCode != 0)
        {
            throw new InvalidOperationException(
                $"{loader} завершил установку с ошибкой (код {process.ExitCode}). {Tail(errorOutput + Environment.NewLine + output)}");
        }

        return FindLoaderProfile(gameDirectory, gameVersion, loader, loaderVersion);
    }

    private static string FindLoaderProfile(
        string gameDirectory,
        string gameVersion,
        string loader,
        string loaderVersion)
    {
        var versionsDirectory = Path.Combine(gameDirectory, "versions");
        if (Directory.Exists(versionsDirectory))
        {
            foreach (var directory in Directory.EnumerateDirectories(versionsDirectory))
            {
                var id = Path.GetFileName(directory);
                var profilePath = Path.Combine(directory, id + ".json");
                if (!File.Exists(profilePath) || !IsSafePathSegment(id))
                {
                    continue;
                }
                using var profile = JsonDocument.Parse(File.ReadAllBytes(profilePath));
                var root = profile.RootElement;
                if (GetNullableString(root, "inheritsFrom") == gameVersion &&
                    id.Contains(loader, StringComparison.OrdinalIgnoreCase) &&
                    id.Contains(loaderVersion, StringComparison.OrdinalIgnoreCase))
                {
                    return id;
                }
            }
        }
        throw new InvalidDataException(
            $"{loader} завершил установку, но его профиль для Minecraft {gameVersion} не найден.");
    }

    private static string Tail(string value) =>
        value.Length <= 4_000 ? value : value[^4_000..];

    private static bool IsSafeSegmentForVersion(string value) =>
        !string.IsNullOrWhiteSpace(value) && value.Length <= 64 &&
        value.All(static character => char.IsAsciiLetterOrDigit(character) || character is '.' or '-' or '_');

    private async Task<Manifest> GetManifestAsync(CancellationToken cancellationToken)
    {
        using var response = await _httpClient.GetAsync(
                VersionManifestUrl, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
            .ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        var manifestBytes = await ReadContentBoundedAsync(response.Content, 16_000_000, cancellationToken)
            .ConfigureAwait(false);
        using var document = JsonDocument.Parse(manifestBytes);
        var versions = document.RootElement.GetProperty("versions").EnumerateArray().Select(item => new VersionEntry(
            RequiredString(item, "id"),
            RequiredString(item, "type"),
            RequiredString(item, "url"))).ToArray();
        var latest = document.RootElement.GetProperty("latest");
        return new Manifest(
            versions,
            latest.GetProperty("release").GetString() ?? string.Empty,
            latest.GetProperty("snapshot").GetString() ?? string.Empty);
    }

    private async Task<DownloadedJson> DownloadJsonAsync(string url, CancellationToken cancellationToken)
    {
        EnsureHttps(url);
        using var response = await _httpClient.GetAsync(url, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
            .ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        if (response.Content.Headers.ContentLength is > 32_000_000)
        {
            throw new InvalidDataException("Ответ сервера Minecraft превышает допустимый размер.");
        }
        var bytes = await ReadContentBoundedAsync(response.Content, 32_000_000, cancellationToken)
            .ConfigureAwait(false);
        return new DownloadedJson(JsonDocument.Parse(bytes), bytes);
    }

    private async Task DownloadVerifiedAsync(
        string url,
        string destination,
        string? sha1,
        CancellationToken cancellationToken)
    {
        EnsureHttps(url);
        if (File.Exists(destination) && sha1 is not null &&
            string.Equals(await GetSha1Async(destination, cancellationToken).ConfigureAwait(false), sha1,
                StringComparison.OrdinalIgnoreCase))
        {
            return;
        }

        var directory = Path.GetDirectoryName(destination)
            ?? throw new InvalidOperationException("Не удалось определить папку загрузки.");
        Directory.CreateDirectory(directory);
        var temporaryPath = destination + "." + Guid.NewGuid().ToString("N") + ".download";
        try
        {
            using var response = await _httpClient.GetAsync(
                    url, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
                .ConfigureAwait(false);
            response.EnsureSuccessStatusCode();
            await using (var input = await response.Content.ReadAsStreamAsync(cancellationToken).ConfigureAwait(false))
            await using (var output = new FileStream(
                             temporaryPath, FileMode.CreateNew, FileAccess.Write, FileShare.None, 81920,
                             FileOptions.Asynchronous | FileOptions.SequentialScan))
            {
                using var hash = sha1 is null ? null : IncrementalHash.CreateHash(HashAlgorithmName.SHA1);
                var buffer = new byte[81920];
                int count;
                while ((count = await input.ReadAsync(buffer, cancellationToken).ConfigureAwait(false)) != 0)
                {
                    hash?.AppendData(buffer, 0, count);
                    await output.WriteAsync(buffer.AsMemory(0, count), cancellationToken).ConfigureAwait(false);
                }
                if (sha1 is not null &&
                    !Convert.ToHexString(hash!.GetHashAndReset()).Equals(sha1, StringComparison.OrdinalIgnoreCase))
                {
                    throw new InvalidDataException($"Контрольная сумма файла Minecraft не совпала: {Path.GetFileName(destination)}");
                }
            }
            File.Move(temporaryPath, destination, overwrite: true);
        }
        finally
        {
            if (File.Exists(temporaryPath))
            {
                File.Delete(temporaryPath);
            }
        }
    }

    private async Task InstallAssetsAsync(
        string gameDirectory,
        string indexPath,
        IProgress<(string Message, int? Percent)>? progress,
        CancellationToken cancellationToken)
    {
        await using var stream = File.OpenRead(indexPath);
        using var document = await JsonDocument.ParseAsync(stream, cancellationToken: cancellationToken)
            .ConfigureAwait(false);
        if (!document.RootElement.TryGetProperty("objects", out var objects))
        {
            throw new InvalidDataException("Индекс игровых ресурсов Minecraft повреждён.");
        }
        var entries = objects.EnumerateObject().Select(static item =>
        {
            var hash = RequiredString(item.Value, "hash");
            if (hash.Length != 40 || hash.Any(static c => !Uri.IsHexDigit(c)))
            {
                throw new InvalidDataException("В индексе Minecraft обнаружен неверный хэш ресурса.");
            }
            return hash;
        }).Distinct(StringComparer.OrdinalIgnoreCase).ToArray();
        var semaphore = new SemaphoreSlim(8);
        var completed = 0;
        await Task.WhenAll(entries.Select(async hash =>
        {
            await semaphore.WaitAsync(cancellationToken).ConfigureAwait(false);
            try
            {
                var relativePath = Path.Combine("assets", "objects", hash[..2], hash);
                var destination = Path.Combine(gameDirectory, relativePath);
                if (!File.Exists(destination) ||
                    !string.Equals(await GetSha1Async(destination, cancellationToken).ConfigureAwait(false), hash,
                        StringComparison.OrdinalIgnoreCase))
                {
                    await DownloadVerifiedAsync(
                            $"https://resources.download.minecraft.net/{hash[..2]}/{hash}",
                            destination,
                            hash,
                            cancellationToken)
                        .ConfigureAwait(false);
                }
                var current = Interlocked.Increment(ref completed);
                if (current % 50 == 0 || current == entries.Length)
                {
                    progress?.Report(($"Загрузка ресурсов {current}/{entries.Length}", 68 + 23 * current / Math.Max(1, entries.Length)));
                }
            }
            finally
            {
                semaphore.Release();
            }
        })).ConfigureAwait(false);
        semaphore.Dispose();
    }

    private static ProcessStartInfo CreateGameStartInfo(
        JsonElement metadata,
        LauncherSettings settings,
        MinecraftAccount? account,
        string version,
        string gameDirectory,
        string assetIndexId,
        string nativesDirectory,
        IReadOnlyList<string> classpath,
        string javaPath)
    {
        var startInfo = new ProcessStartInfo(javaPath)
        {
            WorkingDirectory = gameDirectory,
            UseShellExecute = false,
            CreateNoWindow = false
        };
        var maxMemory = settings.AutoMemory
            ? Math.Clamp((int)(GC.GetGCMemoryInfo().TotalAvailableMemoryBytes / 1024 / 1024 * 0.5), 1024, 8192)
            : settings.MaxMemoryMb;
        startInfo.ArgumentList.Add($"-Xmx{maxMemory}M");
        if (settings.GraphicsProfile == 1)
        {
            startInfo.ArgumentList.Add("-XX:+UseG1GC");
            startInfo.ArgumentList.Add("-XX:+UnlockExperimentalVMOptions");
            startInfo.ArgumentList.Add("-XX:G1NewSizePercent=20");
            startInfo.ArgumentList.Add("-XX:G1ReservePercent=20");
            startInfo.ArgumentList.Add("-XX:MaxGCPauseMillis=50");
            startInfo.ArgumentList.Add("-XX:G1HeapRegionSize=32M");
        }
        if (settings.GarbageCollector == "ZGC" && GetJavaMajorVersion(version) >= 17)
        {
            startInfo.ArgumentList.Add("-XX:+UseZGC");
        }
        else
        {
            startInfo.ArgumentList.Add("-XX:+UseG1GC");
        }

        var variables = new Dictionary<string, string>(StringComparer.Ordinal)
        {
            ["${auth_player_name}"] = account?.Username ?? settings.Username,
            ["${version_name}"] = version,
            ["${game_directory}"] = gameDirectory,
            ["${assets_root}"] = Path.Combine(gameDirectory, "assets"),
            ["${assets_index_name}"] = assetIndexId,
            ["${auth_uuid}"] = account?.Id ?? CreateOfflineUuid(settings.Username),
            ["${auth_access_token}"] = account?.AccessToken ?? "0",
            ["${clientid}"] = string.Empty,
            ["${auth_xuid}"] = account?.Xuid ?? string.Empty,
            ["${user_type}"] = account is null ? "legacy" : "msa",
            ["${version_type}"] = "release",
            ["${natives_directory}"] = nativesDirectory,
            ["${launcher_name}"] = "MintyLauncher",
            ["${launcher_version}"] = "2.2.0",
            ["${classpath}"] = string.Join(Path.PathSeparator, classpath),
            ["${classpath_separator}"] = Path.PathSeparator.ToString(),
            ["${user_properties}"] = "{}"
        };

        if (metadata.TryGetProperty("arguments", out var arguments))
        {
            if (arguments.TryGetProperty("jvm", out var jvm))
            {
                AddArguments(jvm, startInfo, variables);
            }
            else
            {
                startInfo.ArgumentList.Add("-Djava.library.path=" + nativesDirectory);
                startInfo.ArgumentList.Add("-cp");
                startInfo.ArgumentList.Add(string.Join(Path.PathSeparator, classpath));
            }
        }
        else
        {
            startInfo.ArgumentList.Add("-Djava.library.path=" + nativesDirectory);
            startInfo.ArgumentList.Add("-cp");
            startInfo.ArgumentList.Add(string.Join(Path.PathSeparator, classpath));
        }

        startInfo.ArgumentList.Add(metadata.GetProperty("mainClass").GetString()
            ?? throw new InvalidDataException("В описании Minecraft отсутствует mainClass."));
        if (settings.Fullscreen)
        {
            startInfo.ArgumentList.Add("--fullscreen");
        }
        if (metadata.TryGetProperty("minecraftArguments", out var legacyArguments))
        {
            foreach (var argument in legacyArguments.GetString()?.Split(' ', StringSplitOptions.RemoveEmptyEntries) ?? [])
            {
                startInfo.ArgumentList.Add(ReplaceVariables(argument, variables));
            }
            if (settings.Fullscreen)
            {
                startInfo.ArgumentList.Add("--fullscreen");
            }
        }
        else
        {
            if (metadata.TryGetProperty("arguments", out var gameArguments) &&
                gameArguments.TryGetProperty("game", out var game))
            {
                AddArguments(game, startInfo, variables);
            }
        }
        return startInfo;
    }

    private static void AddArguments(
        JsonElement arguments,
        ProcessStartInfo startInfo,
        IReadOnlyDictionary<string, string> variables)
    {
        foreach (var argument in arguments.EnumerateArray())
        {
            if (argument.ValueKind == JsonValueKind.String)
            {
                startInfo.ArgumentList.Add(ReplaceVariables(argument.GetString()!, variables));
            }
            else if (argument.ValueKind == JsonValueKind.Object &&
                     IsAllowedByRules(argument, "windows", Environment.Is64BitOperatingSystem ? "x86_64" : "x86"))
            {
                var value = argument.GetProperty("value");
                if (value.ValueKind == JsonValueKind.String)
                {
                    startInfo.ArgumentList.Add(ReplaceVariables(value.GetString()!, variables));
                }
                else if (value.ValueKind == JsonValueKind.Array)
                {
                    foreach (var part in value.EnumerateArray())
                    {
                        startInfo.ArgumentList.Add(ReplaceVariables(part.GetString()!, variables));
                    }
                }
            }
        }
    }

    private static string ReplaceVariables(string value, IReadOnlyDictionary<string, string> variables)
    {
        foreach (var pair in variables)
        {
            value = value.Replace(pair.Key, pair.Value, StringComparison.Ordinal);
        }
        return value;
    }

    private static string CreateOfflineUuid(string username)
    {
        var bytes = MD5.HashData(Encoding.UTF8.GetBytes("Offline:" + username));
        bytes[6] = (byte)((bytes[6] & 0x0F) | 0x30);
        bytes[8] = (byte)((bytes[8] & 0x3F) | 0x80);
        return Convert.ToHexString(bytes).ToLowerInvariant();
    }

    private static bool IsAllowedByRules(JsonElement element, string osName, string osArch)
    {
        if (!element.TryGetProperty("rules", out var rules))
        {
            return true;
        }
        var allowed = false;
        foreach (var rule in rules.EnumerateArray())
        {
            var action = RequiredString(rule, "action");
            var matches = true;
            if (rule.TryGetProperty("os", out var os))
            {
                if (os.TryGetProperty("name", out var name) && name.GetString() != osName)
                {
                    matches = false;
                }
                if (os.TryGetProperty("arch", out var arch) && arch.GetString() != osArch)
                {
                    matches = false;
                }
                if (os.TryGetProperty("version", out var version) &&
                    !Regex.IsMatch(Environment.OSVersion.Version.ToString(), version.GetString() ?? "(?!)"))
                {
                    matches = false;
                }
            }
            if (matches && rule.TryGetProperty("features", out var features))
            {
                foreach (var feature in features.EnumerateObject())
                {
                    var enabled = feature.Name switch
                    {
                        "is_demo_user" or "has_custom_resolution" or "is_quick_play_singleplayer" or
                            "is_quick_play_multiplayer" or "is_quick_play_realms" or "has_quick_plays_support" => false,
                        _ => false
                    };
                    if (feature.Value.ValueKind != JsonValueKind.True &&
                        feature.Value.ValueKind != JsonValueKind.False)
                    {
                        matches = false;
                        break;
                    }
                    if (feature.Value.GetBoolean() != enabled)
                    {
                        matches = false;
                        break;
                    }
                }
            }
            allowed = matches ? action == "allow" : allowed;
        }
        return allowed;
    }

    private static string ResolveLibraryPath(string librariesDirectory, string relativePath)
    {
        var normalized = relativePath.Replace('/', Path.DirectorySeparatorChar);
        if (Path.IsPathRooted(normalized) || normalized.Split(Path.DirectorySeparatorChar).Any(part => part == ".."))
        {
            throw new InvalidDataException("Описание Minecraft содержит небезопасный путь к библиотеке.");
        }
        return Path.Combine(librariesDirectory, normalized);
    }

    private static string ResolveMavenArtifact(string coordinate)
    {
        var extension = "jar";
        var extensionSeparator = coordinate.LastIndexOf('@');
        if (extensionSeparator >= 0)
        {
            extension = coordinate[(extensionSeparator + 1)..];
            coordinate = coordinate[..extensionSeparator];
        }
        var parts = coordinate.Split(':');
        if (parts.Length is < 3 or > 4 ||
            parts.Any(static part => part.Length == 0 || part.Contains('/') || part.Contains('\\')) ||
            parts[0].Split('.').Any(static part =>
                part.Length == 0 || !part.All(static character => char.IsAsciiLetterOrDigit(character) || character is '-' or '_')) ||
            extension.Length == 0 || !extension.All(static character => char.IsAsciiLetterOrDigit(character)))
        {
            throw new InvalidDataException("Профиль Minecraft содержит неверную Maven-зависимость.");
        }
        var groupPath = parts[0].Replace('.', Path.DirectorySeparatorChar);
        var artifactName = parts[1];
        var artifactVersion = parts[2];
        var classifier = parts.Length == 4 ? "-" + parts[3] : string.Empty;
        var filename = $"{artifactName}-{artifactVersion}{classifier}.{extension}";
        return Path.Combine(groupPath, artifactName, artifactVersion, filename);
    }

    private static void ExtractNativeJar(string jarPath, string destinationDirectory)
    {
        using var archive = ZipFile.OpenRead(jarPath);
        foreach (var entry in archive.Entries)
        {
            if (string.IsNullOrEmpty(entry.Name) || entry.FullName.StartsWith("META-INF/", StringComparison.OrdinalIgnoreCase))
            {
                continue;
            }
            var relativePath = entry.FullName.Replace('/', Path.DirectorySeparatorChar);
            if (Path.IsPathRooted(relativePath) || relativePath.Split(Path.DirectorySeparatorChar).Any(part => part == ".."))
            {
                throw new InvalidDataException("Java-библиотека содержит небезопасный путь.");
            }
            var destination = Path.Combine(destinationDirectory, relativePath);
            Directory.CreateDirectory(Path.GetDirectoryName(destination)!);
            entry.ExtractToFile(destination, overwrite: true);
        }
    }

    private async Task<string> ResolveJavaAsync(
        string configuredPath,
        string gameVersion,
        CancellationToken cancellationToken)
    {
        var candidates = new List<string>();
        if (!string.IsNullOrWhiteSpace(configuredPath))
        {
            candidates.Add(configuredPath);
        }
        if (!string.IsNullOrWhiteSpace(Environment.GetEnvironmentVariable("JAVA_HOME")))
        {
            var home = Environment.GetEnvironmentVariable("JAVA_HOME")!;
            candidates.Add(Path.Combine(home, "bin", "javaw.exe"));
            candidates.Add(Path.Combine(home, "bin", "java.exe"));
        }
        var path = Environment.GetEnvironmentVariable("PATH") ?? string.Empty;
        candidates.AddRange(path.Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries)
            .Select(directory => Path.Combine(directory, OperatingSystem.IsWindows() ? "javaw.exe" : "java")));
        foreach (var candidate in candidates)
        {
            if (File.Exists(candidate))
            {
                return candidate;
            }
        }

        var major = GetJavaMajorVersion(gameVersion);
        var runtimeDirectory = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
            "MintyLauncher", "runtimes", $"java-{major}");
        var javaExecutable = Path.Combine(runtimeDirectory, "bin", OperatingSystem.IsWindows() ? "javaw.exe" : "java");
        if (File.Exists(javaExecutable))
        {
            return javaExecutable;
        }

        if (!OperatingSystem.IsWindows())
        {
            throw new FileNotFoundException(
                $"Для Minecraft {gameVersion} требуется Java {major}. Установите Java и укажите путь в настройках.");
        }
        cancellationToken.ThrowIfCancellationRequested();
        Directory.CreateDirectory(runtimeDirectory);
        var architecture = System.Runtime.InteropServices.RuntimeInformation.ProcessArchitecture switch
        {
            System.Runtime.InteropServices.Architecture.X64 => "x64",
            System.Runtime.InteropServices.Architecture.Arm64 => "aarch64",
            _ => throw new PlatformNotSupportedException("Автоматическая установка Java поддерживает Windows x64 и ARM64.")
        };
        var archivePath = Path.Combine(runtimeDirectory, "runtime.zip");
        var downloadUrl = $"https://api.adoptium.net/v3/binary/latest/{major}/ga/windows/{architecture}/jre/hotspot/normal/eclipse";
        await DownloadVerifiedAsync(downloadUrl, archivePath, null, cancellationToken).ConfigureAwait(false);
        ZipFile.ExtractToDirectory(archivePath, runtimeDirectory, overwriteFiles: true);
        File.Delete(archivePath);
        var nestedJava = Directory.EnumerateFiles(runtimeDirectory, "javaw.exe", SearchOption.AllDirectories)
            .FirstOrDefault()
            ?? throw new InvalidDataException("Установленный Java Runtime не содержит javaw.exe.");
        var nestedBin = Path.GetDirectoryName(nestedJava)!;
        var nestedRoot = Path.GetDirectoryName(nestedBin)!;
        if (!string.Equals(nestedRoot, runtimeDirectory, StringComparison.OrdinalIgnoreCase))
        {
            var normalizedRuntime = runtimeDirectory.TrimEnd(Path.DirectorySeparatorChar) + Path.DirectorySeparatorChar;
            foreach (var entry in Directory.EnumerateFileSystemEntries(nestedRoot))
            {
                var destination = Path.Combine(runtimeDirectory, Path.GetFileName(entry));
                if (!destination.StartsWith(normalizedRuntime, StringComparison.OrdinalIgnoreCase))
                {
                    throw new InvalidDataException("Архив Java содержит небезопасный путь.");
                }
                if (Directory.Exists(entry))
                {
                    Directory.Move(entry, destination);
                }
                else
                {
                    File.Move(entry, destination, overwrite: true);
                }
            }
        }
        if (!File.Exists(javaExecutable))
        {
            throw new InvalidDataException("Не удалось завершить установку Java Runtime.");
        }
        return javaExecutable;
    }

    private async Task<string> SaveLoaderProfileAsync(
        string gameDirectory,
        string gameVersion,
        DownloadedJson profile,
        string loader,
        CancellationToken cancellationToken)
    {
        var id = RequiredString(profile.RootElement, "id");
        if (!IsSafePathSegment(id) ||
            GetNullableString(profile.RootElement, "inheritsFrom") != gameVersion)
        {
            throw new InvalidDataException($"Профиль {loader} не наследует выбранную версию Minecraft.");
        }
        var directory = Path.Combine(gameDirectory, "versions", id);
        Directory.CreateDirectory(directory);
        var destination = Path.Combine(directory, id + ".json");
        await File.WriteAllBytesAsync(destination, Encoding.UTF8.GetBytes(profile.RootElement.GetRawText()), cancellationToken)
            .ConfigureAwait(false);
        return id;
    }

    private static void EnsureHttps(string url)
    {
        if (!Uri.TryCreate(url, UriKind.Absolute, out var uri) || uri.Scheme != Uri.UriSchemeHttps)
        {
            throw new InvalidDataException("Сервер Minecraft вернул небезопасную ссылку.");
        }
    }

    private static bool IsSafePathSegment(string value) =>
        !string.IsNullOrWhiteSpace(value) &&
        value is not "." and not ".." &&
        value.IndexOfAny(Path.GetInvalidFileNameChars()) < 0 &&
        !value.Contains('/') && !value.Contains('\\') &&
        !value.Any(char.IsControl);

    private static int GetJavaMajorVersion(string gameVersion)
    {
        var parts = gameVersion.Split('.');
        if (parts.Length < 2 || !int.TryParse(parts[1], out var minor))
        {
            return 21;
        }
        var patch = parts.Length > 2 && int.TryParse(parts[2], out var parsedPatch) ? parsedPatch : 0;
        return minor switch
        {
            <= 16 => 8,
            17 => 16,
            18 or 19 => 17,
            20 when patch <= 4 => 17,
            _ => 21
        };
    }

    private static async Task<string> GetSha1Async(string path, CancellationToken cancellationToken)
    {
        await using var stream = File.OpenRead(path);
        var hash = await SHA1.HashDataAsync(stream, cancellationToken).ConfigureAwait(false);
        return Convert.ToHexString(hash);
    }

    private static async Task<byte[]> ReadContentBoundedAsync(
        HttpContent content,
        int maximumBytes,
        CancellationToken cancellationToken)
    {
        if (content.Headers.ContentLength is { } contentLength && contentLength > maximumBytes)
        {
            throw new InvalidDataException("Ответ сервера Minecraft превышает допустимый размер.");
        }
        await using var input = await content.ReadAsStreamAsync(cancellationToken).ConfigureAwait(false);
        using var output = new MemoryStream();
        var buffer = new byte[81920];
        int count;
        while ((count = await input.ReadAsync(buffer, cancellationToken).ConfigureAwait(false)) != 0)
        {
            if (output.Length + count > maximumBytes)
            {
                throw new InvalidDataException("Ответ сервера Minecraft превышает допустимый размер.");
            }
            await output.WriteAsync(buffer.AsMemory(0, count), cancellationToken).ConfigureAwait(false);
        }
        return output.ToArray();
    }

    private static string RequiredString(JsonElement element, string property) =>
        GetNullableString(element, property)
        ?? throw new InvalidDataException($"В ответе Minecraft отсутствует поле {property}.");

    private static string? GetNullableString(JsonElement element, string property) =>
        element.ValueKind == JsonValueKind.Object &&
        element.TryGetProperty(property, out var value) &&
        value.ValueKind == JsonValueKind.String
            ? value.GetString()
            : null;

    private static HttpClient CreateClient()
    {
        var client = new HttpClient { Timeout = TimeSpan.FromMinutes(10) };
        client.DefaultRequestHeaders.UserAgent.ParseAdd(
            "MintyLauncher/2.2.0 (https://github.com/ZeeinJordansky/Launcher-minecraft-AI)");
        return client;
    }

    public void Dispose()
    {
        if (_ownsClient)
        {
            _httpClient.Dispose();
        }
    }

    private sealed record VersionEntry(string Id, string Type, string Url);
    private sealed record Manifest(VersionEntry[] Versions, string LatestRelease, string LatestSnapshot);
    private sealed class DownloadedJson(JsonDocument document, byte[] raw) : IDisposable
    {
        public JsonElement RootElement => document.RootElement;
        public byte[] Raw => raw;

        public void Dispose() => document.Dispose();
    }
}
