using System.Diagnostics;
using System.IO;
using System.Text.Json;
using MintyLauncher.Models;

namespace MintyLauncher.Services;

public sealed class EngineClient
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    public async Task<EngineEvent?> ExecuteAsync(
        string action,
        LauncherSettings? settings = null,
        IProgress<EngineEvent>? progress = null,
        CancellationToken cancellationToken = default)
    {
        return await ExecuteRequestAsync(new EngineRequest(action, settings), progress, cancellationToken);
    }

    public Task<EngineEvent?> SearchModsAsync(
        string query,
        string category,
        string sort,
        LauncherSettings settings,
        IProgress<EngineEvent>? progress = null,
        CancellationToken cancellationToken = default)
    {
        var request = new EngineRequest("search_mods", settings, query, category,
            settings.SelectedVersion, settings.ModLoader, sort, string.Empty);
        return ExecuteRequestAsync(request, progress, cancellationToken);
    }

    public Task<EngineEvent?> InstallModAsync(
        string slug,
        string category,
        LauncherSettings settings,
        IProgress<EngineEvent>? progress = null,
        CancellationToken cancellationToken = default)
    {
        var request = new EngineRequest("install_mod", settings, string.Empty, category,
            settings.SelectedVersion, settings.ModLoader, string.Empty, slug);
        return ExecuteRequestAsync(request, progress, cancellationToken);
    }

    private async Task<EngineEvent?> ExecuteRequestAsync(
        EngineRequest request,
        IProgress<EngineEvent>? progress,
        CancellationToken cancellationToken)
    {
        var startInfo = CreateStartInfo();
        startInfo.RedirectStandardInput = true;
        startInfo.RedirectStandardOutput = true;
        startInfo.RedirectStandardError = true;

        using var process = new Process { StartInfo = startInfo, EnableRaisingEvents = true };
        if (!process.Start())
        {
            throw new InvalidOperationException("Не удалось запустить Go-движок Minty.");
        }

        using var cancellationRegistration = cancellationToken.Register(() =>
        {
            try
            {
                if (!process.HasExited)
                {
                    process.Kill(entireProcessTree: true);
                }
            }
            catch (InvalidOperationException)
            {
            }
        });

        var stderrTask = process.StandardError.ReadToEndAsync(cancellationToken);
        EngineEvent? lastEvent = null;
        string? engineError = null;
        var stdoutTask = Task.Run(async () =>
        {
            while (await process.StandardOutput.ReadLineAsync(cancellationToken) is { } line)
            {
                if (string.IsNullOrWhiteSpace(line))
                {
                    continue;
                }

                EngineEvent? engineEvent;
                try
                {
                    engineEvent = JsonSerializer.Deserialize<EngineEvent>(line, JsonOptions);
                }
                catch (JsonException exception)
                {
                    engineError = $"Go-движок вернул некорректное событие: {exception.Message}";
                    continue;
                }

                if (engineEvent is null)
                {
                    continue;
                }

                lastEvent = engineEvent;
                if (engineEvent.Type == "error")
                {
                    engineError = engineEvent.Message;
                }
                progress?.Report(engineEvent);
            }
        }, cancellationToken);

        var payload = JsonSerializer.Serialize(request, JsonOptions);
        await process.StandardInput.WriteLineAsync(payload);
        process.StandardInput.Close();
        await process.WaitForExitAsync(cancellationToken);
        await stdoutTask;
        var stderr = await stderrTask;

        if (process.ExitCode != 0)
        {
            throw new InvalidOperationException(engineError ??
                (string.IsNullOrWhiteSpace(stderr) ? "Go-движок завершился с ошибкой." : stderr.Trim()));
        }
        return lastEvent;
    }

    private static ProcessStartInfo CreateStartInfo()
    {
        var packagedEngine = Path.Combine(AppContext.BaseDirectory, "minty-engine.exe");
        if (File.Exists(packagedEngine))
        {
            return new ProcessStartInfo(packagedEngine)
            {
                WorkingDirectory = AppContext.BaseDirectory,
                UseShellExecute = false,
                CreateNoWindow = true
            };
        }

        var directory = new DirectoryInfo(AppContext.BaseDirectory);
        while (directory is not null)
        {
            var engineDirectory = Path.Combine(directory.FullName, "engine");
            if (File.Exists(Path.Combine(engineDirectory, "go.mod")))
            {
                var startInfo = new ProcessStartInfo("go")
                {
                    WorkingDirectory = engineDirectory,
                    UseShellExecute = false,
                    CreateNoWindow = true
                };
                startInfo.ArgumentList.Add("run");
                startInfo.ArgumentList.Add("./cmd/minty-engine");
                return startInfo;
            }
            directory = directory.Parent;
        }

        throw new FileNotFoundException("Не найден minty-engine.exe или исходники Go-движка.");
    }

    private sealed record EngineRequest(
        string Action,
        LauncherSettings? Settings,
        string Query = "",
        string Category = "",
        string Version = "",
        string Loader = "",
        string Sort = "",
        string Slug = "");
}

public sealed class EngineEvent
{
    public string Type { get; set; } = string.Empty;
    public string Message { get; set; } = string.Empty;
    public string Version { get; set; } = string.Empty;
    public IReadOnlyList<EngineVersion> Versions { get; set; } = [];
    public double Progress { get; set; }
    public long DownloadedBytes { get; set; }
    public long TotalBytes { get; set; }
    public double BytesPerSecond { get; set; }
    public long EtaSeconds { get; set; }
    public LauncherSettings? Settings { get; set; }
    public IReadOnlyList<EngineProject> Projects { get; set; } = [];
}

public sealed class EngineVersion
{
    public string Id { get; set; } = string.Empty;
    public string Type { get; set; } = string.Empty;
    public string ReleaseTime { get; set; } = string.Empty;
}

public sealed class EngineProject
{
    public string Slug { get; set; } = string.Empty;
    public string Title { get; set; } = string.Empty;
    public string Description { get; set; } = string.Empty;
    public string Author { get; set; } = string.Empty;
    public long Downloads { get; set; }
    public string IconUrl { get; set; } = string.Empty;
    public string ProjectType { get; set; } = string.Empty;
    public IReadOnlyList<string> Categories { get; set; } = [];
}
