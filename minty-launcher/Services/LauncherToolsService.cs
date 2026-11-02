using System.IO.Compression;
using System.Runtime.InteropServices;
using MintyLauncher.Models;

namespace MintyLauncher.Services;

public sealed class LauncherToolsService
{
    public IReadOnlyList<string> GetProfiles(LauncherSettings settings)
    {
        var profiles = settings.InstalledProfiles
            .Where(IsSafeSegment)
            .Distinct(StringComparer.OrdinalIgnoreCase)
            .ToList();
        if (!profiles.Contains("default", StringComparer.OrdinalIgnoreCase))
        {
            profiles.Insert(0, "default");
        }
        return profiles;
    }

    public string ResolveGameDirectory(LauncherSettings settings)
    {
        var root = Path.GetFullPath(settings.GameDirectory);
        if (!settings.IsolatedProfiles || settings.SelectedProfileId == "default")
        {
            return root;
        }
        if (!IsSafeSegment(settings.SelectedProfileId))
        {
            throw new InvalidDataException("Указан неверный идентификатор профиля.");
        }
        return Path.Combine(root, "profiles", settings.SelectedProfileId);
    }

    public void AddProfile(LauncherSettings settings, string profileName)
    {
        if (!IsSafeSegment(profileName))
        {
            throw new ArgumentException("Название профиля должно содержать только буквы, цифры, дефис или подчёркивание.");
        }
        var profileIds = GetProfiles(settings).ToList();
        if (profileIds.Contains(profileName, StringComparer.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException("Профиль с таким названием уже существует.");
        }
        profileIds.Add(profileName);
        settings.InstalledProfiles = profileIds.ToArray();
        settings.SelectedProfileId = profileName;
        Directory.CreateDirectory(ResolveGameDirectory(settings));
    }

    public IReadOnlyList<WorldEntry> GetWorlds(string gameDirectory)
    {
        var savesPath = Path.Combine(gameDirectory, "saves");
        if (!Directory.Exists(savesPath))
        {
            return [];
        }
        return Directory.EnumerateDirectories(savesPath)
            .Select(path => new DirectoryInfo(path))
            .Where(directory => File.Exists(Path.Combine(directory.FullName, "level.dat")))
            .OrderByDescending(static directory => directory.LastWriteTimeUtc)
            .Select(static directory => new WorldEntry(
                directory.Name, directory.FullName, directory.LastWriteTime))
            .ToArray();
    }

    public async Task<string> ImportWorldAsync(
        string archivePath,
        string gameDirectory,
        CancellationToken cancellationToken = default)
    {
        if (!File.Exists(archivePath))
        {
            throw new FileNotFoundException("Архив мира не найден.", archivePath);
        }
        var savesDirectory = Path.Combine(gameDirectory, "saves");
        var stagingDirectory = Path.Combine(savesDirectory, ".minty-import-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(stagingDirectory);
        try
        {
            using (var archive = ZipFile.OpenRead(archivePath))
            {
                if (archive.Entries.Count > 100_000)
                {
                    throw new InvalidDataException("Архив мира содержит слишком много файлов.");
                }
                long totalExpandedBytes = 0;
                foreach (var entry in archive.Entries)
                {
                    cancellationToken.ThrowIfCancellationRequested();
                    var relativePath = entry.FullName.Replace('/', Path.DirectorySeparatorChar);
                    if (Path.IsPathRooted(relativePath) ||
                        relativePath.Split(Path.DirectorySeparatorChar).Any(part => part is ".." or "."))
                    {
                        throw new InvalidDataException("Архив мира содержит небезопасный путь.");
                    }
                    totalExpandedBytes = checked(totalExpandedBytes + entry.Length);
                    if (totalExpandedBytes > 4L * 1024 * 1024 * 1024)
                    {
                        throw new InvalidDataException("Размер распакованного мира превышает 4 ГБ.");
                    }
                    var destination = Path.GetFullPath(Path.Combine(stagingDirectory, relativePath));
                    if (!destination.StartsWith(
                            Path.GetFullPath(stagingDirectory) + Path.DirectorySeparatorChar,
                            OperatingSystem.IsWindows() ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal))
                    {
                        throw new InvalidDataException("Архив мира пытается выйти за пределы папки импорта.");
                    }
                    if (string.IsNullOrEmpty(entry.Name))
                    {
                        Directory.CreateDirectory(destination);
                        continue;
                    }
                    Directory.CreateDirectory(Path.GetDirectoryName(destination)!);
                    await using var input = entry.Open();
                    await using var output = new FileStream(
                        destination, FileMode.CreateNew, FileAccess.Write, FileShare.None, 81920,
                        FileOptions.Asynchronous | FileOptions.SequentialScan);
                    await input.CopyToAsync(output, cancellationToken).ConfigureAwait(false);
                }
            }

            var worldRoot = Directory.EnumerateDirectories(stagingDirectory)
                .FirstOrDefault(path => File.Exists(Path.Combine(path, "level.dat")));
            if (worldRoot is null && !File.Exists(Path.Combine(stagingDirectory, "level.dat")))
            {
                throw new InvalidDataException("В архиве не найден файл level.dat — это не похоже на мир Minecraft.");
            }
            worldRoot ??= stagingDirectory;
            Directory.CreateDirectory(savesDirectory);
            var baseName = Path.GetFileName(worldRoot == stagingDirectory ? archivePath : worldRoot);
            if (worldRoot != stagingDirectory)
            {
                baseName = Path.GetFileName(worldRoot);
            }
            baseName = SanitizeName(Path.GetFileNameWithoutExtension(baseName));
            var destinationPath = Path.Combine(savesDirectory, baseName);
            var suffix = 2;
            while (Directory.Exists(destinationPath))
            {
                destinationPath = Path.Combine(savesDirectory, $"{baseName} ({suffix++})");
            }
            Directory.Move(worldRoot, destinationPath);
            return destinationPath;
        }
        finally
        {
            if (Directory.Exists(stagingDirectory))
            {
                Directory.Delete(stagingDirectory, recursive: true);
            }
        }
    }

    public async Task<string> ExportWorldAsync(
        WorldEntry world,
        string archivePath,
        CancellationToken cancellationToken = default)
    {
        if (!File.Exists(Path.Combine(world.Directory, "level.dat")))
        {
            throw new InvalidDataException("Не найден обязательный файл level.dat.");
        }
        var worldDirectory = Path.GetFullPath(world.Directory);
        var fullArchivePath = Path.GetFullPath(archivePath);
        if (fullArchivePath.StartsWith(
                worldDirectory.TrimEnd(Path.DirectorySeparatorChar) + Path.DirectorySeparatorChar,
                OperatingSystem.IsWindows() ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal))
        {
            throw new InvalidOperationException("Архив резервной копии нельзя сохранить внутри папки мира.");
        }
        archivePath = fullArchivePath;
        await using var output = new FileStream(
            archivePath, FileMode.Create, FileAccess.Write, FileShare.None, 81920, FileOptions.Asynchronous);
        using var archive = new ZipArchive(output, ZipArchiveMode.Create);
        foreach (var filePath in Directory.EnumerateFiles(world.Directory, "*", SearchOption.AllDirectories))
        {
            cancellationToken.ThrowIfCancellationRequested();
            var relativePath = Path.GetRelativePath(world.Directory, filePath);
            var entry = archive.CreateEntry(relativePath.Replace(Path.DirectorySeparatorChar, '/'));
            await using var source = File.OpenRead(filePath);
            await using var target = entry.Open();
            await source.CopyToAsync(target, cancellationToken).ConfigureAwait(false);
        }
        return archivePath;
    }

    public IReadOnlyList<ModFileEntry> GetMods(string gameDirectory)
    {
        var modsDirectory = Path.Combine(gameDirectory, "mods");
        if (!Directory.Exists(modsDirectory))
        {
            return [];
        }
        return Directory.EnumerateFiles(modsDirectory)
            .Select(path => new FileInfo(path))
            .OrderBy(static file => file.Name, StringComparer.OrdinalIgnoreCase)
            .Select(static file => new ModFileEntry(
                file.Name, file.FullName, file.Length, file.Extension.Equals(".jar", StringComparison.OrdinalIgnoreCase)))
            .ToArray();
    }

    public void RemoveMod(string gameDirectory, ModFileEntry mod)
    {
        var path = Path.GetFullPath(mod.Path);
        var modsDirectory = Path.GetFullPath(Path.Combine(gameDirectory, "mods"));
        if (!string.Equals(Path.GetDirectoryName(path), modsDirectory, OperatingSystem.IsWindows()
                ? StringComparison.OrdinalIgnoreCase
                : StringComparison.Ordinal))
        {
            throw new InvalidOperationException("Некорректный путь к файлу мода.");
        }
        File.Delete(path);
    }

    public async Task<LauncherDiagnostics> CollectDiagnosticsAsync(
        string gameDirectory,
        string javaPath,
        CancellationToken cancellationToken = default)
    {
        var warnings = new List<string>();
        var memoryMb = GC.GetGCMemoryInfo().TotalAvailableMemoryBytes / 1024 / 1024;
        if (memoryMb < 4096)
        {
            warnings.Add("Объёма оперативной памяти меньше 4 ГБ — используйте лёгкие сборки и уменьшите настройки графики.");
        }
        if (string.IsNullOrWhiteSpace(javaPath) && string.IsNullOrWhiteSpace(Environment.GetEnvironmentVariable("JAVA_HOME")))
        {
            warnings.Add("Путь к Java не задан. Лаунчер установит подходящую Java автоматически при первом запуске.");
        }
        var crashDirectory = Path.Combine(gameDirectory, "crash-reports");
        var latestCrash = Directory.Exists(crashDirectory)
            ? Directory.EnumerateFiles(crashDirectory, "*.txt").OrderByDescending(File.GetLastWriteTimeUtc).FirstOrDefault()
            : null;
        var logsDirectory = Path.Combine(gameDirectory, "logs");
        var latestLog = Directory.Exists(logsDirectory)
            ? Directory.EnumerateFiles(logsDirectory, "*.log").OrderByDescending(File.GetLastWriteTimeUtc).FirstOrDefault()
            : null;
        if (latestCrash is not null)
        {
            var info = new FileInfo(latestCrash);
            if (info.Length > 10 * 1024 * 1024)
            {
                warnings.Add("Последний crash-report слишком велик для автоматического анализа.");
            }
            else
            {
                var report = await File.ReadAllTextAsync(latestCrash, cancellationToken).ConfigureAwait(false);
                warnings.AddRange(AnalyzeCrash(report));
            }
        }
        return new LauncherDiagnostics(
            RuntimeInformation.OSDescription,
            RuntimeInformation.ProcessArchitecture.ToString(),
            Environment.ProcessorCount,
            memoryMb,
            javaPath,
            latestCrash,
            latestLog,
            warnings);
    }

    public static bool IsSafeSegment(string value) =>
        !string.IsNullOrWhiteSpace(value) && value is not "." and not ".." &&
        value.Length <= 40 &&
        value.All(static character => char.IsAsciiLetterOrDigit(character) || character is '-' or '_');

    private static string SanitizeName(string value)
    {
        var sanitized = string.Concat(value.Select(character =>
            char.IsAsciiLetterOrDigit(character) || character is ' ' or '-' or '_' ? character : '_')).Trim();
        if (string.IsNullOrWhiteSpace(sanitized))
        {
            throw new InvalidDataException("Не удалось определить имя мира.");
        }
        return sanitized.Length <= 80 ? sanitized : sanitized[..80];
    }

    private static IEnumerable<string> AnalyzeCrash(string report)
    {
        if (report.Contains("OutOfMemoryError", StringComparison.OrdinalIgnoreCase))
        {
            yield return "Похоже, Minecraft исчерпал доступную память. Попробуйте увеличить лимит памяти или отключить тяжёлые шейдеры.";
        }
        if (report.Contains("Mixin apply failed", StringComparison.OrdinalIgnoreCase) ||
            report.Contains("MixinApplyError", StringComparison.OrdinalIgnoreCase))
        {
            yield return "Ошибка Mixin часто указывает на несовместимый или устаревший мод. Проверьте последние установленные моды.";
        }
        if (report.Contains("UnsupportedClassVersionError", StringComparison.OrdinalIgnoreCase))
        {
            yield return "Для этой версии Minecraft используется неподходящая Java. Запустите игру через автоматический подбор Java.";
        }
    }
}
