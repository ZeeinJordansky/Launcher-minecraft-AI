using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using MintyLauncher.Models;

namespace MintyLauncher.Services;

public sealed class SecureUpdateService(HttpClient? httpClient = null) : IDisposable
{
    private const string RepositoryApi =
        "https://api.github.com/repos/ZeeinJordansky/Launcher-minecraft-AI";
    private const string ReleaseDownload =
        "https://github.com/ZeeinJordansky/Launcher-minecraft-AI/releases/download";
    private readonly HttpClient _httpClient = httpClient ?? CreateClient();
    private readonly bool _ownsClient = httpClient is null;

    public async Task<SecureUpdateResult> DownloadLatestInstallerAsync(
        string currentVersion,
        CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(UpdateSigningKey.PublicKeyBase64))
        {
            throw new InvalidOperationException(
                "Безопасное обновление отключено: в сборку не встроен доверенный ключ подписи.");
        }

        using var releaseResponse = await _httpClient.GetAsync(
                RepositoryApi + "/releases/latest",
                HttpCompletionOption.ResponseHeadersRead,
                cancellationToken)
            .ConfigureAwait(false);
        releaseResponse.EnsureSuccessStatusCode();
        await using var releaseStream = await releaseResponse.Content.ReadAsStreamAsync(cancellationToken)
            .ConfigureAwait(false);
        using var release = await JsonDocument.ParseAsync(
                releaseStream, cancellationToken: cancellationToken)
            .ConfigureAwait(false);
        var tag = RequiredString(release.RootElement, "tag_name");
        if (!tag.StartsWith('v') || !Version.TryParse(tag[1..], out var releaseVersion))
        {
            throw new InvalidDataException("GitHub вернул некорректную версию релиза.");
        }

        var manifestUrl = $"{ReleaseDownload}/{Uri.EscapeDataString(tag)}/MintyUpdateManifest.json";
        var signatureUrl = $"{ReleaseDownload}/{Uri.EscapeDataString(tag)}/MintyUpdateManifest.sig";
        var manifestBytes = await DownloadBytesAsync(manifestUrl, 1_000_000, cancellationToken)
            .ConfigureAwait(false);
        var signatureText = Encoding.ASCII.GetString(
            await DownloadBytesAsync(signatureUrl, 4_096, cancellationToken).ConfigureAwait(false)).Trim();
        byte[] signature;
        try
        {
            signature = Convert.FromBase64String(signatureText);
        }
        catch (FormatException exception)
        {
            throw new InvalidDataException("Подпись обновления имеет неверный формат.", exception);
        }

        if (!UpdateManifestSignatureVerifier.Verify(
                manifestBytes, signature, UpdateSigningKey.PublicKeyBase64))
        {
            throw new CryptographicException("Подпись обновления недействительна. Файл не будет установлен.");
        }

        using var manifest = JsonDocument.Parse(manifestBytes);
        var manifestVersion = RequiredString(manifest.RootElement, "version");
        if (!string.Equals(manifestVersion, tag[1..], StringComparison.Ordinal) ||
            !Version.TryParse(manifestVersion, out var signedVersion) ||
            signedVersion != releaseVersion)
        {
            throw new InvalidDataException("Версия в подписанном манифесте не совпадает с GitHub Release.");
        }

        var isCurrent = Version.TryParse(currentVersion, out var installedVersion) &&
                        releaseVersion <= installedVersion;
        var assets = manifest.RootElement.GetProperty("assets");
        var expectedHash = assets.EnumerateArray()
            .Where(item => RequiredString(item, "name") == "MintyLauncher-Setup.exe")
            .Select(item => RequiredString(item, "sha256"))
            .SingleOrDefault()
            ?? throw new InvalidDataException("В подписанном манифесте нет установщика Minty.");
        if (expectedHash.Length != 64 || expectedHash.Any(static character => !Uri.IsHexDigit(character)))
        {
            throw new InvalidDataException("Подписанный манифест содержит неверную контрольную сумму.");
        }
        if (isCurrent)
        {
            return new SecureUpdateResult(releaseVersion.ToString(), string.Empty, true);
        }

        var outputDirectory = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
            "MintyLauncher", "updates");
        Directory.CreateDirectory(outputDirectory);
        var installerPath = Path.Combine(outputDirectory, $"MintyLauncher-Setup-{releaseVersion}.exe");
        var temporaryPath = installerPath + "." + Guid.NewGuid().ToString("N") + ".download";
        var installerUrl =
            $"{ReleaseDownload}/{Uri.EscapeDataString(tag)}/MintyLauncher-Setup.exe";
        try
        {
            await DownloadVerifiedInstallerAsync(installerUrl, temporaryPath, expectedHash, cancellationToken)
                .ConfigureAwait(false);
            File.Move(temporaryPath, installerPath, overwrite: true);
        }
        finally
        {
            if (File.Exists(temporaryPath))
            {
                File.Delete(temporaryPath);
            }
        }

        return new SecureUpdateResult(releaseVersion.ToString(), installerPath, isCurrent);
    }

    private async Task DownloadVerifiedInstallerAsync(
        string url,
        string destination,
        string expectedHash,
        CancellationToken cancellationToken)
    {
        using var response = await _httpClient.GetAsync(
                url, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
            .ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        if (response.Content.Headers.ContentLength is > 1_000_000_000)
        {
            throw new InvalidDataException("Размер установщика превышает допустимый предел.");
        }
        await using var input = await response.Content.ReadAsStreamAsync(cancellationToken).ConfigureAwait(false);
        await using var output = new FileStream(
            destination, FileMode.CreateNew, FileAccess.Write, FileShare.None, 81920,
            FileOptions.Asynchronous | FileOptions.SequentialScan);
        using var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        var buffer = new byte[81920];
        long totalBytes = 0;
        int read;
        while ((read = await input.ReadAsync(buffer, cancellationToken).ConfigureAwait(false)) != 0)
        {
            totalBytes = checked(totalBytes + read);
            if (totalBytes > 1_000_000_000)
            {
                throw new InvalidDataException("Размер установщика превышает допустимый предел.");
            }
            hash.AppendData(buffer, 0, read);
            await output.WriteAsync(buffer.AsMemory(0, read), cancellationToken).ConfigureAwait(false);
        }
        if (!Convert.ToHexString(hash.GetHashAndReset()).Equals(expectedHash, StringComparison.OrdinalIgnoreCase))
        {
            throw new CryptographicException("Хэш установщика не совпал с подписанным манифестом.");
        }
    }

    private async Task<byte[]> DownloadBytesAsync(
        string url,
        int maximumBytes,
        CancellationToken cancellationToken)
    {
        using var response = await _httpClient.GetAsync(url, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
            .ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        if (response.Content.Headers.ContentLength is { } contentLength && contentLength > maximumBytes)
        {
            throw new InvalidDataException("Файл обновления превышает допустимый размер.");
        }
        await using var stream = await response.Content.ReadAsStreamAsync(cancellationToken).ConfigureAwait(false);
        using var buffer = new MemoryStream();
        var chunk = new byte[4096];
        int read;
        while ((read = await stream.ReadAsync(chunk, cancellationToken).ConfigureAwait(false)) != 0)
        {
            if (buffer.Length + read > maximumBytes)
            {
                throw new InvalidDataException("Файл обновления превышает допустимый размер.");
            }
            await buffer.WriteAsync(chunk.AsMemory(0, read), cancellationToken).ConfigureAwait(false);
        }
        return buffer.ToArray();
    }

    private static string RequiredString(JsonElement element, string property) =>
        element.TryGetProperty(property, out var value) && value.ValueKind == JsonValueKind.String
            ? value.GetString() ?? throw new InvalidDataException($"В манифесте отсутствует {property}.")
            : throw new InvalidDataException($"В манифесте отсутствует {property}.");

    private static HttpClient CreateClient()
    {
        var client = new HttpClient { Timeout = TimeSpan.FromMinutes(10) };
        client.DefaultRequestHeaders.UserAgent.ParseAdd(
            "MintyLauncher/2.1.0 (https://github.com/ZeeinJordansky/Launcher-minecraft-AI)");
        return client;
    }

    public void Dispose()
    {
        if (_ownsClient)
        {
            _httpClient.Dispose();
        }
    }
}
