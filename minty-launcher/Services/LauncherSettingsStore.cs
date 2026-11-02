using System.Text.Json;
using MintyLauncher.Models;

namespace MintyLauncher.Services;

public sealed class LauncherSettingsStore
{
    private static readonly string SettingsPath = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
        "MintyLauncher",
        "settings.json");

    public async Task<LauncherSettings> LoadAsync(CancellationToken cancellationToken = default)
    {
        if (!File.Exists(SettingsPath))
        {
            return new LauncherSettings();
        }

        try
        {
            await using var stream = File.OpenRead(SettingsPath);
            return await JsonSerializer.DeserializeAsync(
                    stream,
                    LauncherSettingsJsonContext.Default.LauncherSettings,
                    cancellationToken)
                .ConfigureAwait(false)
                ?? throw new InvalidDataException("Файл настроек Minty пуст.");
        }
        catch (JsonException exception)
        {
            throw new InvalidDataException("Не удалось прочитать настройки Minty. Проверьте файл settings.json.", exception);
        }
    }

    public async Task SaveAsync(LauncherSettings settings, CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(settings);
        if (settings.MaxMemoryMb is < 512 or > 131072)
        {
            throw new ArgumentOutOfRangeException(nameof(settings), "Память JVM должна быть от 512 до 131072 МБ.");
        }

        var directory = Path.GetDirectoryName(SettingsPath)
            ?? throw new InvalidOperationException("Не удалось определить папку настроек Minty.");
        Directory.CreateDirectory(directory);
        var temporaryPath = SettingsPath + "." + Guid.NewGuid().ToString("N") + ".tmp";
        await using (var stream = new FileStream(
                         temporaryPath, FileMode.CreateNew, FileAccess.Write, FileShare.None, 4096,
                         FileOptions.Asynchronous | FileOptions.WriteThrough))
        {
            await JsonSerializer.SerializeAsync(
                    stream,
                    settings,
                    LauncherSettingsJsonContext.Default.LauncherSettings,
                    cancellationToken)
                .ConfigureAwait(false);
            await stream.FlushAsync(cancellationToken).ConfigureAwait(false);
        }
        File.Move(temporaryPath, SettingsPath, overwrite: true);
    }
}
