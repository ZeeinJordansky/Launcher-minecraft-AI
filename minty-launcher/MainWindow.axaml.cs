using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Controls.Templates;
using Avalonia.Input;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Threading;
using Avalonia.VisualTree;
using MintyLauncher.Models;
using MintyLauncher.Services;
using System.Diagnostics;
using System.Security.Cryptography;
using System.Text.Json;

namespace MintyLauncher;

public sealed partial class MainWindow : Window
{
    private readonly LauncherSettingsStore _settingsStore = new();
    private readonly LauncherToolsService _tools = new();
    private readonly MinecraftLauncherService _minecraft = new();
    private readonly ModrinthService _modrinth = new();
    private readonly MinecraftAuthenticationService _authentication = new();
    private readonly SecureUpdateService _updates = new();
    private LauncherSettings _settings = new();
    private string? _selectedSkinPath;
    private Avalonia.Media.Imaging.Bitmap? _skinBitmap;
    private TextBox? _modQuery;
    private ComboBox? _modContentType;
    private ComboBox? _modSort;
    private StackPanel? _modResults;
    private TextBlock? _modResultStatus;
    private static readonly string[] Navigation =
    [
        "home", "instances", "versions", "mods", "worlds", "skin",
        "bedrock", "accounts", "diagnostics", "settings"
    ];

    private static readonly string[] PageNames =
    [
        "Главная", "Сборки", "Все версии Minecraft", "Каталог Modrinth",
        "Миры и ресурсы", "Скины", "Bedrock Edition", "Аккаунты",
        "Диагностика", "Настройки"
    ];

    private static readonly (string Name, string Color, string Hover, string Foreground)[] AccentPalette =
    [
        ("Синий", "#7C8CE8", "#9AA6F0", "#111319"),
        ("Фиолетовый", "#B18AE8", "#C4A5EF", "#17111F"),
        ("Бирюзовый", "#55C6C1", "#76D5D0", "#101A1A"),
        ("Мятный", "#75C99D", "#91D8B1", "#111A14"),
        ("Оранжевый", "#E6A65C", "#F0B977", "#21170D"),
        ("Розовый", "#E58BB8", "#EDA5C8", "#21131B"),
        ("Красный", "#E87979", "#F09292", "#211111")
    ];

    public MainWindow()
    {
        InitializeComponent();
        Closed += (_, _) =>
        {
            _authentication.Dispose();
            _minecraft.Dispose();
            _modrinth.Dispose();
            _updates.Dispose();
            _skinBitmap?.Dispose();
        };
        Opened += async (_, _) =>
        {
            try
            {
                _settings = await _settingsStore.LoadAsync();
                EnsureInstanceSettings();
                LoadSelectedInstanceSettings();
                ApplyAccent();
                Classes.Set("no-animations", !_settings.EnableAnimations);
                AccountBadge.Text = string.IsNullOrWhiteSpace(_settings.AccountId)
                    ? $"ОФЛАЙН • {_settings.AccountName}"
                    : $"MICROSOFT • {_settings.AccountName}";
                ShowPage("home");
            }
            catch (InvalidDataException exception)
            {
                SetStatus("Не удалось загрузить настройки", exception.Message);
            }
            catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
            {
                SetStatus("Не удалось открыть настройки", exception.Message);
            }
        };
        ShowPage("home");
    }

    private void Navigate_Click(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (sender is Button { Tag: string page })
        {
            ShowPage(page);
            foreach (var button in FindControls<Button>(this))
            {
                if (button.Classes.Contains("nav"))
                {
                    button.Classes.Set("active", Equals(button.Tag, page));
                }
            }
        }
    }

    private void ShowPage(string page)
    {
        var index = Array.IndexOf(Navigation, page);
        if (index < 0)
        {
            return;
        }

        PageEyebrow.Text = $"MINTY LAUNCHER  /  {index + 1:00}";
        PageTitle.Text = PageNames[index];
        foreach (var button in FindControls<Button>(this))
        {
            if (button.Classes.Contains("nav"))
            {
                button.Classes.Set("active", Equals(button.Tag, page));
            }
        }
        PageHost.Children.Clear();
        PageHost.Children.Add(page switch
        {
            "home" => BuildHome(),
            "instances" => BuildInstances(),
            "versions" => BuildVersions(),
            "mods" => BuildMods(),
            "worlds" => BuildWorlds(),
            "skin" => BuildSkin(),
            "bedrock" => BuildBedrock(),
            "accounts" => BuildAccounts(),
            "diagnostics" => BuildDiagnostics(),
            "settings" => BuildSettings(),
            _ => BuildHome()
        });
        if (page == "mods")
        {
            _ = RunUiActionAsync(LoadModCatalogAsync);
        }
    }

    private Control BuildHome()
    {
        var root = new StackPanel { Spacing = 18, MaxWidth = 920 };
        root.Children.Add(new TextBlock
        {
            Text = "Готов к игре?",
            FontSize = 30,
            FontWeight = FontWeight.Bold
        });
        root.Children.Add(new TextBlock
        {
            Text = "Выбери сборку и версию. Остальные инструменты находятся в разделах слева.",
            FontSize = 13,
            Foreground = Brush.Parse("#A5A8AF"),
            TextWrapping = TextWrapping.Wrap,
            Margin = new Thickness(0, -10, 0, 2)
        });
        var gameCard = new Border
        {
            Padding = new Thickness(24),
            CornerRadius = new CornerRadius(16),
            Background = Brush.Parse("#222429"),
            BorderBrush = Brush.Parse("#373A41"),
            BorderThickness = new Thickness(1),
            Child = new StackPanel { Spacing = 14 }
        };
        var gameContent = (StackPanel)gameCard.Child!;
        gameContent.Children.Add(new TextBlock
        {
            Text = "ТЕКУЩАЯ СБОРКА",
            FontSize = 10,
            FontWeight = FontWeight.Bold,
            Foreground = Brush.Parse("#92959D"),
            LetterSpacing = 1
        });
        gameContent.Children.Add(new TextBlock
        {
            Text = _settings.SelectedProfileId == "default"
                ? "Основная сборка"
                : _settings.SelectedProfileId,
            FontSize = 21,
            FontWeight = FontWeight.Bold
        });
        var selectedVersionLabel = _settings.SelectedVersion switch
        {
            "latest-release" => "Последний релиз",
            "latest-snapshot" => "Последний снапшот",
            _ => _settings.SelectedVersion
        };
        var currentVersion = new TextBlock
        {
            Text = $"{selectedVersionLabel}  ·  {_settings.ModLoader}",
            FontSize = 13,
            Foreground = Brush.Parse("#B7BAC1")
        };
        gameContent.Children.Add(currentVersion);
        var actions = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 10,
            Margin = new Thickness(0, 6, 0, 0)
        };
        actions.Children.Add(CreatePlayButton());
        actions.Children.Add(CreatePageButton("Выбрать версию", "secondary", "versions"));
        actions.Children.Add(CreatePageButton("Сборки", "secondary", "instances"));
        gameContent.Children.Add(actions);
        root.Children.Add(gameCard);

        var quickLinks = new UniformGrid { Columns = 3, Rows = 1 };
        quickLinks.Children.Add(CreatePageButton("Каталог модов", "secondary", "mods"));
        quickLinks.Children.Add(CreatePageButton("Миры", "secondary", "worlds"));
        quickLinks.Children.Add(CreatePageButton("Настройки", "secondary", "settings"));
        root.Children.Add(quickLinks);
        return root;
    }

    private Control BuildInstances()
    {
        var panel = CreatePage("Сборки", "У каждой сборки свои версия Minecraft, загрузчик, моды, миры и папка игры.");
        var profilePicker = CreateCombo(_tools.GetProfiles(_settings), _settings.SelectedProfileId);
        profilePicker.SelectionChanged += async (_, _) =>
        {
            if (profilePicker.SelectedItem is string profile)
            {
                await SaveSettingsAsync();
                _settings.SelectedProfileId = profile;
                LoadSelectedInstanceSettings();
                await SaveSettingsAsync();
                ShowPage("instances");
            }
        };
        panel.Children.Add(profilePicker);
        var newProfile = CreateTextBox("Название новой сборки");
        panel.Children.Add(newProfile);
        panel.Children.Add(CreateActionButton("Создать изолированную сборку", async () =>
        {
            await SaveSettingsAsync();
            _settings.IsolatedProfiles = true;
            _tools.AddProfile(_settings, newProfile.Text ?? string.Empty);
            _settings.Instances[_settings.SelectedProfileId] = new MinecraftInstanceSettings
            {
                GameVersion = _settings.SelectedVersion,
                ModLoader = "Vanilla",
                ModLoaderVersion = string.Empty,
                SelectedLoaderProfileId = string.Empty
            };
            LoadSelectedInstanceSettings();
            await SaveSettingsAsync();
            ShowPage("instances");
            SetStatus("Сборка создана", _tools.ResolveGameDirectory(_settings));
        }));
        var loaderPicker = CreateCombo(
            ["Vanilla", "Fabric", "Quilt", "Forge", "NeoForge"],
            _settings.ModLoader);
        loaderPicker.SelectionChanged += async (_, _) =>
        {
            if (loaderPicker.SelectedItem is string loader)
            {
                _settings.ModLoader = loader;
                _settings.SelectedLoaderProfileId = string.Empty;
                await SaveSettingsAsync();
            }
        };
        panel.Children.Add(loaderPicker);
        var loaderVersion = CreateTextBox("Версия загрузчика (пусто — последняя)");
        loaderVersion.Text = _settings.ModLoaderVersion;
        panel.Children.Add(loaderVersion);
        panel.Children.Add(CreateActionButton("Установить загрузчик в выбранную сборку", async () =>
        {
            var gameDirectory = _tools.ResolveGameDirectory(_settings);
            var version = await ResolveSelectedVersionAsync();
            var installedId = _settings.ModLoader switch
            {
                "Fabric" => await _minecraft.InstallFabricAsync(gameDirectory, version, loaderVersion.Text ?? string.Empty),
                "Quilt" => await _minecraft.InstallQuiltAsync(gameDirectory, version, loaderVersion.Text ?? string.Empty),
                "Forge" => await _minecraft.InstallForgeAsync(gameDirectory, version, loaderVersion.Text ?? string.Empty),
                "NeoForge" => await _minecraft.InstallNeoForgeAsync(gameDirectory, version, loaderVersion.Text ?? string.Empty),
                "Vanilla" => version,
                _ => throw new NotSupportedException($"Неизвестный загрузчик: {_settings.ModLoader}.")
            };
            _settings.SelectedLoaderProfileId = installedId;
            _settings.ModLoaderVersion = loaderVersion.Text ?? string.Empty;
            await SaveSettingsAsync();
            SetStatus("Загрузчик установлен", installedId);
        }));
        panel.Children.Add(CreateActionButton("Удалить выбранную сборку из списка", async () =>
        {
            var profileId = _settings.SelectedProfileId;
            if (profileId.Equals("default", StringComparison.OrdinalIgnoreCase))
            {
                throw new InvalidOperationException("Основную сборку удалить нельзя.");
            }
            _settings.InstalledProfiles = _tools.GetProfiles(_settings)
                .Where(profile => !profile.Equals(profileId, StringComparison.OrdinalIgnoreCase))
                .ToArray();
            _settings.Instances.Remove(profileId);
            _settings.SelectedProfileId = "default";
            LoadSelectedInstanceSettings();
            await SaveSettingsAsync();
            ShowPage("instances");
            SetStatus("Сборка удалена из списка", "Файлы сборки сохранены на диске.");
        }));
        return panel;
    }

    private StackPanel CreatePage(string title, string description)
    {
        var panel = new StackPanel { Spacing = 14, MaxWidth = 960 };
        panel.Children.Add(new TextBlock { Text = title, FontSize = 30, FontWeight = FontWeight.Bold });
        panel.Children.Add(new TextBlock
        {
            Text = description,
            FontSize = 13,
            Foreground = Brush.Parse("#A5A8AF"),
            TextWrapping = TextWrapping.Wrap,
            MaxWidth = 760
        });
        return panel;
    }

    private Button CreatePlayButton()
    {
        var button = CreateButton("Играть", "primary");
        button.Click += async (_, _) => await RunUiActionAsync(async () =>
        {
            MinecraftAccount? account = null;
            if (!string.IsNullOrWhiteSpace(_settings.AccountId))
            {
                SetStatus("Обновляю данные аккаунта", "Проверка токена и владения Minecraft.");
                account = await _authentication.RefreshAsync();
                _settings.AccountName = account.Username;
                _settings.AccountId = account.Id;
                AccountBadge.Text = $"MICROSOFT • {account.Username}";
            }
            else
            {
                _settings.Username = _settings.AccountName;
            }
            await SaveSettingsAsync();
            var baseGameDirectory = _settings.GameDirectory;
            _settings.GameDirectory = _tools.ResolveGameDirectory(_settings);
            var reporter = new Progress<(string Message, int? Percent)>(item =>
            {
                FooterStatus.Text = item.Message;
                if (item.Percent.HasValue)
                {
                    FooterDetails.Text = $"{item.Percent.Value}%";
                }
            });
            try
            {
                await _minecraft.InstallAndLaunchAsync(_settings, account, reporter);
            }
            finally
            {
                _settings.GameDirectory = baseGameDirectory;
            }
            SetStatus("Minecraft запущен", $"{_settings.SelectedVersion} · {_settings.ModLoader}");
            if (_settings.CloseAfterGame)
            {
                Close();
            }
        });
        return button;
    }

    private Button CreatePageButton(string label, string style, string page, Func<Task>? action = null)
    {
        var button = CreateButton(label, style);
        button.HorizontalAlignment = HorizontalAlignment.Stretch;
        button.Click += async (_, _) =>
        {
            if (action is null)
            {
                ShowPage(page);
            }
            else
            {
                await RunUiActionAsync(action);
            }
        };
        return button;
    }

    private Button CreateActionButton(string label, Func<Task> action) =>
        CreatePageButton(label, "secondary", string.Empty, action);

    private static TextBox CreateTextBox(string watermark) =>
        new()
        {
            PlaceholderText = watermark,
            MinHeight = 42,
            Padding = new Thickness(12, 8),
            CornerRadius = new CornerRadius(10),
            Background = Brush.Parse("#202227"),
            Foreground = Brushes.White
        };

    private static ComboBox CreateCombo(
        IEnumerable<string> values,
        string selected,
        Action<string>? onSelection = null)
    {
        var combo = new ComboBox
        {
            ItemsSource = values.ToArray(),
            SelectedItem = selected,
            MinHeight = 42
        };
        if (onSelection is not null)
        {
            combo.SelectionChanged += (_, _) =>
            {
                if (combo.SelectedItem is string value)
                {
                    onSelection(value);
                }
            };
        }
        return combo;
    }

    private async Task RunUiActionAsync(Func<Task> action)
    {
        try
        {
            await action();
        }
        catch (Exception exception) when (exception is HttpRequestException or IOException or
                                           InvalidOperationException or UnauthorizedAccessException or
                                           NotSupportedException or TimeoutException or OperationCanceledException or
                                           JsonException or CryptographicException or ArgumentException or
                                           FormatException or OverflowException or System.ComponentModel.Win32Exception)
        {
            SetStatus("Не удалось выполнить действие", exception.Message);
        }
    }

    private async Task SaveSettingsAsync()
    {
        CaptureSelectedInstanceSettings();
        await _settingsStore.SaveAsync(_settings);
    }

    private void EnsureInstanceSettings()
    {
        var storedInstances = _settings.Instances ?? [];
        var instances = new Dictionary<string, MinecraftInstanceSettings>(
            StringComparer.OrdinalIgnoreCase);
        foreach (var profileId in _tools.GetProfiles(_settings))
        {
            if (storedInstances.TryGetValue(profileId, out var saved) && saved is not null)
            {
                instances[profileId] = saved;
                continue;
            }

            var isSelected = profileId.Equals(
                _settings.SelectedProfileId, StringComparison.OrdinalIgnoreCase);
            instances[profileId] = new MinecraftInstanceSettings
            {
                GameVersion = isSelected ? _settings.SelectedVersion : "latest-release",
                ModLoader = isSelected ? _settings.ModLoader : "Vanilla",
                ModLoaderVersion = isSelected ? _settings.ModLoaderVersion : string.Empty,
                SelectedLoaderProfileId = isSelected ? _settings.SelectedLoaderProfileId : string.Empty
            };
        }

        _settings.Instances = instances;
        if (!_settings.Instances.ContainsKey(_settings.SelectedProfileId))
        {
            _settings.SelectedProfileId = "default";
        }
    }

    private void LoadSelectedInstanceSettings()
    {
        if (!_settings.Instances.TryGetValue(_settings.SelectedProfileId, out var instance))
        {
            throw new InvalidDataException("Настройки выбранной сборки не найдены.");
        }
        _settings.SelectedVersion = instance.GameVersion;
        _settings.ModLoader = instance.ModLoader;
        _settings.ModLoaderVersion = instance.ModLoaderVersion;
        _settings.SelectedLoaderProfileId = instance.SelectedLoaderProfileId;
    }

    private void CaptureSelectedInstanceSettings()
    {
        if (_settings.Instances.TryGetValue(_settings.SelectedProfileId, out var instance))
        {
            instance.GameVersion = _settings.SelectedVersion;
            instance.ModLoader = _settings.ModLoader;
            instance.ModLoaderVersion = _settings.ModLoaderVersion;
            instance.SelectedLoaderProfileId = _settings.SelectedLoaderProfileId;
        }
    }

    private void ApplyAccent()
    {
        var resources = Application.Current?.Resources;
        if (resources is null)
        {
            return;
        }

        var option = AccentPalette.FirstOrDefault(item =>
            item.Color.Equals(_settings.AccentColor, StringComparison.OrdinalIgnoreCase));
        var color = Color.Parse(_settings.AccentColor);
        var customForeground = color.R * 0.299 + color.G * 0.587 + color.B * 0.114 > 150
            ? "#111319"
            : "#F1F2F4";
        resources["AccentBrush"] = Brush.Parse(_settings.AccentColor);
        resources["AccentHoverBrush"] = option.Hover is not null
            ? Brush.Parse(option.Hover)
            : new SolidColorBrush(Color.FromRgb(
                (byte)Math.Min(255, color.R + 20),
                (byte)Math.Min(255, color.G + 20),
                (byte)Math.Min(255, color.B + 20)));
        resources["AccentForegroundBrush"] = Brush.Parse(
            option.Foreground ?? customForeground);
    }

    private async Task<string> ResolveSelectedVersionAsync()
    {
        if (_settings.SelectedVersion is not ("latest-release" or "latest-snapshot"))
        {
            return _settings.SelectedVersion;
        }
        var versions = await _minecraft.GetVersionsAsync();
        var preferredType = _settings.SelectedVersion == "latest-snapshot" ? "snapshot" : "release";
        return versions.FirstOrDefault(version => version.Type == preferredType)?.Id
               ?? throw new InvalidOperationException("Не найдена подходящая версия Minecraft.");
    }

    private void SetStatus(string status, string details)
    {
        FooterStatus.Text = status;
        FooterDetails.Text = details;
    }

    private Task OpenFolderAsync(string path)
    {
        Directory.CreateDirectory(path);
        var startInfo = OperatingSystem.IsWindows()
            ? new ProcessStartInfo("explorer.exe") { UseShellExecute = false }
            : new ProcessStartInfo("xdg-open") { UseShellExecute = false };
        startInfo.ArgumentList.Add(path);
        Process.Start(startInfo);
        return Task.CompletedTask;
    }

    private static string? GetLocalPath(Avalonia.Platform.Storage.IStorageItem? item) =>
        item?.Path is { IsFile: true } fileUri ? fileUri.LocalPath : null;

    private Control BuildVersions()
    {
        var panel = CreatePage("Все версии Minecraft", "Полный официальный каталог Mojang: релизы, снапшоты, бета- и альфа-версии.");
        var search = CreateTextBox("Фильтр по номеру версии");
        var typeFilter = CreateCombo(
            ["Все типы", "Релизы", "Снэпшоты", "Бета", "Альфа"],
            "Все типы");
        var versionStatus = new TextBlock
        {
            Text = "Загружаю каталог Mojang…",
            FontSize = 11,
            Foreground = Brush.Parse("#A5A8AF")
        };
        var versionList = new ListBox
        {
            MaxHeight = 560,
            MinHeight = 240,
            ItemTemplate = new FuncDataTemplate<MinecraftVersion>((version, _) =>
                version is null
                    ? null
                    : new Grid
                    {
                        ColumnDefinitions = new ColumnDefinitions("*,Auto"),
                        Margin = new Thickness(4, 2),
                        Children =
                        {
                            new TextBlock
                            {
                                Text = version.Id,
                                FontSize = 13,
                                VerticalAlignment = VerticalAlignment.Center
                            },
                            CreateVersionTypeBadge(version.Type)
                        }
                    })
        };
        var filterRow = new Grid { ColumnDefinitions = new ColumnDefinitions("*,220"), ColumnSpacing = 10 };
        filterRow.Children.Add(search);
        Grid.SetColumn(typeFilter, 1);
        filterRow.Children.Add(typeFilter);
        panel.Children.Add(filterRow);
        panel.Children.Add(versionStatus);
        panel.Children.Add(versionList);

        IReadOnlyList<MinecraftVersion> allVersions = [];
        var updatingSelection = false;
        void ApplyVersionFilters()
        {
            var searchText = search.Text?.Trim();
            var type = typeFilter.SelectedItem?.ToString();
            var matchingVersions = allVersions.Where(version =>
                (string.IsNullOrEmpty(searchText) ||
                 version.Id.Contains(searchText, StringComparison.OrdinalIgnoreCase)) &&
                (type is null or "Все типы" ||
                 type switch
                 {
                     "Релизы" => version.Type == "release",
                     "Снэпшоты" => version.Type == "snapshot",
                     "Бета" => version.Type == "old_beta",
                     "Альфа" => version.Type == "old_alpha",
                     _ => true
                 })).ToArray();
            updatingSelection = true;
            versionList.ItemsSource = matchingVersions;
            var selectedVersionId = _settings.SelectedVersion switch
            {
                "latest-release" => allVersions.FirstOrDefault(version => version.Type == "release")?.Id,
                "latest-snapshot" => allVersions.FirstOrDefault(version => version.Type == "snapshot")?.Id,
                _ => _settings.SelectedVersion
            };
            versionList.SelectedItem = matchingVersions.FirstOrDefault(version =>
                version.Id == selectedVersionId);
            updatingSelection = false;
            versionStatus.Text = allVersions.Count == 0
                ? "Каталог ещё не загружен."
                : $"Показано {matchingVersions.Length:N0} из {allVersions.Count:N0} версий";
        }

        search.TextChanged += (_, _) => ApplyVersionFilters();
        typeFilter.SelectionChanged += (_, _) => ApplyVersionFilters();
        versionList.SelectionChanged += async (_, _) =>
        {
            if (updatingSelection || versionList.SelectedItem is not MinecraftVersion selected)
            {
                return;
            }
            _settings.SelectedVersion = selected.Id;
            await SaveSettingsAsync();
            SetStatus("Версия выбрана", selected.Id);
        };
        _ = RunUiActionAsync(async () =>
        {
            allVersions = await _minecraft.GetVersionsAsync();
            ApplyVersionFilters();
            SetStatus("Каталог загружен", $"{allVersions.Count:N0} версий Mojang");
        });
        return panel;
    }

    private Control BuildMods()
    {
        var panel = CreatePage("Каталог Modrinth", "Популярные проекты загружаются автоматически. Поиск, сортировка, фильтр игры и проверенная установка.");
        _modQuery = CreateTextBox("Поиск модов, ресурспаков и шейдеров");
        _modContentType = CreateCombo(["Моды", "Ресурспаки", "Шейдеры"], "Моды");
        _modSort = CreateCombo(
            ["Популярные", "По релевантности", "Новые", "Недавно обновлённые"],
            "Популярные");
        _modResults = new StackPanel { Spacing = 8 };
        _modResultStatus = new TextBlock
        {
            Text = "Загружаю популярные проекты…",
            FontSize = 11,
            Foreground = Brush.Parse("#A5A8AF")
        };
        var searchRow = new Grid { ColumnDefinitions = new ColumnDefinitions("*,Auto"), ColumnSpacing = 10 };
        searchRow.Children.Add(_modQuery);
        var searchButton = CreateButton("Найти", "primary");
        searchButton.Click += async (_, _) => await RunUiActionAsync(LoadModCatalogAsync);
        Grid.SetColumn(searchButton, 1);
        searchRow.Children.Add(searchButton);
        var filters = new Grid { ColumnDefinitions = new ColumnDefinitions("*,*"), ColumnSpacing = 10 };
        filters.Children.Add(_modContentType);
        Grid.SetColumn(_modSort, 1);
        filters.Children.Add(_modSort);
        panel.Children.Add(searchRow);
        panel.Children.Add(filters);
        panel.Children.Add(_modResultStatus);
        panel.Children.Add(new ScrollViewer
        {
            MaxHeight = 500,
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            Content = _modResults
        });
        _modQuery.KeyDown += async (_, eventArgs) =>
        {
            if (eventArgs.Key == Key.Enter)
            {
                await RunUiActionAsync(LoadModCatalogAsync);
            }
        };
        _modContentType.SelectionChanged += async (_, _) =>
            await RunUiActionAsync(LoadModCatalogAsync);
        _modSort.SelectionChanged += async (_, _) =>
            await RunUiActionAsync(LoadModCatalogAsync);

        var installedMods = new StackPanel { Spacing = 8 };
        panel.Children.Add(new TextBlock
        {
            Text = "УСТАНОВЛЕННЫЕ МОДЫ",
            FontSize = 10,
            FontWeight = FontWeight.Bold,
            Foreground = Brush.Parse(_settings.AccentColor),
            Margin = new Thickness(0, 10, 0, 0)
        });
        foreach (var mod in _tools.GetMods(_tools.ResolveGameDirectory(_settings)))
        {
            installedMods.Children.Add(CreatePageButton($"{mod.Name}   ·   {mod.Size / 1024:N0} КБ", "secondary", "mods",
                () =>
                {
                    _tools.RemoveMod(_tools.ResolveGameDirectory(_settings), mod);
                    ShowPage("mods");
                    SetStatus("Мод удалён", mod.Name);
                    return Task.CompletedTask;
                }));
        }
        panel.Children.Add(installedMods);
        return panel;
    }

    private async Task LoadModCatalogAsync()
    {
        if (_modQuery is null || _modContentType is null || _modSort is null ||
            _modResults is null || _modResultStatus is null)
        {
            throw new InvalidOperationException("Каталог модов ещё не подготовлен.");
        }

        _modResultStatus.Text = "Загружаю каталог Modrinth…";
        _modResults.Children.Clear();
        var projectType = _modContentType.SelectedItem switch
        {
            "Ресурспаки" => "resourcepack",
            "Шейдеры" => "shader",
            _ => "mod"
        };
        var sortIndex = _modSort.SelectedItem switch
        {
            "По релевантности" => "relevance",
            "Новые" => "newest",
            "Недавно обновлённые" => "updated",
            _ => "downloads"
        };
        var projects = await _modrinth.SearchAsync(
            _modQuery.Text?.Trim() ?? string.Empty,
            "all",
            await ResolveSelectedVersionAsync(),
            _settings.ModLoader,
            projectType,
            sortIndex);

        foreach (var project in projects)
        {
            var content = new StackPanel { Spacing = 7 };
            content.Children.Add(new TextBlock
            {
                Text = project.Title,
                FontSize = 15,
                FontWeight = FontWeight.Bold,
                TextWrapping = TextWrapping.Wrap
            });
            content.Children.Add(new TextBlock
            {
                Text = $"{project.Description}   ·   {project.Downloads:N0} загрузок",
                FontSize = 11,
                Foreground = Brush.Parse("#A5A8AF"),
                TextWrapping = TextWrapping.Wrap
            });
            var installButton = CreateActionButton("Установить", async () =>
            {
                var path = await _modrinth.InstallAsync(
                    project.Id,
                    _tools.ResolveGameDirectory(_settings),
                    await ResolveSelectedVersionAsync(),
                    _settings.ModLoader,
                    projectType);
                SetStatus("Контент установлен", Path.GetFileName(path));
            });
            installButton.IsEnabled = projectType != "mod" ||
                                     _settings.ModLoader is "Fabric" or "Forge" or "NeoForge" or "Quilt";
            if (!installButton.IsEnabled)
            {
                installButton.Content = "Для установки выбери загрузчик в разделе «Сборки»";
            }
            content.Children.Add(installButton);
            _modResults.Children.Add(new Border
            {
                Padding = new Thickness(14),
                CornerRadius = new CornerRadius(10),
                Background = Brush.Parse("#222429"),
                BorderBrush = Brush.Parse("#373A41"),
                BorderThickness = new Thickness(1),
                Child = content
            });
        }
        _modResultStatus.Text = projects.Count == 0
            ? "Ничего не найдено. Измени запрос или фильтр."
            : $"{projects.Count} проектов · версия {_settings.SelectedVersion} · {_settings.ModLoader}";
        SetStatus("Каталог готов", $"Проектов: {projects.Count}");
    }

    private static Control CreateVersionTypeBadge(string type)
    {
        var label = type switch
        {
            "release" => "РЕЛИЗ",
            "snapshot" => "СНЭПШОТ",
            "old_beta" => "БЕТА",
            "old_alpha" => "АЛЬФА",
            _ => type.ToUpperInvariant()
        };
        var badge = new TextBlock
        {
            Text = label,
            FontSize = 9,
            Foreground = Brush.Parse("#A5A8AF"),
            VerticalAlignment = VerticalAlignment.Center,
            HorizontalAlignment = HorizontalAlignment.Right
        };
        Grid.SetColumn(badge, 1);
        return badge;
    }

    private Control BuildWorlds()
    {
        var panel = CreatePage("Миры твоего Minecraft", "Импорт ZIP-архивов, просмотр и безопасное резервное копирование.");
        panel.Children.Add(CreateActionButton("Импортировать мир из ZIP", async () =>
        {
            var files = await StorageProvider.OpenFilePickerAsync(new Avalonia.Platform.Storage.FilePickerOpenOptions
            {
                Title = "Выбери архив мира",
                AllowMultiple = false,
                FileTypeFilter = [new Avalonia.Platform.Storage.FilePickerFileType("Архив ZIP") { Patterns = ["*.zip"] }]
            });
            var path = GetLocalPath(files.FirstOrDefault());
            if (path is null)
            {
                return;
            }
            var imported = await _tools.ImportWorldAsync(path, _tools.ResolveGameDirectory(_settings));
            SetStatus("Мир импортирован", Path.GetFileName(imported));
            ShowPage("worlds");
        }));
        foreach (var world in _tools.GetWorlds(_tools.ResolveGameDirectory(_settings)))
        {
            panel.Children.Add(CreatePageButton(
                $"{world.Name}   ·   {world.LastModified:g}",
                "secondary",
                "worlds",
                async () =>
                {
                    var target = await StorageProvider.SaveFilePickerAsync(new Avalonia.Platform.Storage.FilePickerSaveOptions
                    {
                        Title = "Сохранить резервную копию мира",
                        SuggestedFileName = world.Name + ".zip",
                        FileTypeChoices = [new Avalonia.Platform.Storage.FilePickerFileType("Архив ZIP") { Patterns = ["*.zip"] }]
                    });
                    var path = GetLocalPath(target);
                    if (path is not null)
                    {
                        await _tools.ExportWorldAsync(world, path);
                        SetStatus("Резервная копия создана", path);
                    }
                }));
        }
        panel.Children.Add(CreateActionButton("Открыть папку игры", () => OpenFolderAsync(_tools.ResolveGameDirectory(_settings))));
        return panel;
    }

    private Control BuildSkin()
    {
        var panel = CreatePage("Скины", "Импортируй PNG-скин и сохрани его в своей библиотеке.");
        var preview = new Border
        {
            Width = 180,
            Height = 180,
            CornerRadius = new CornerRadius(24),
            Background = Brush.Parse("#292B31"),
            Child = new TextBlock
            {
                Text = "M",
                FontSize = 110,
                FontWeight = FontWeight.Black,
                Foreground = Brush.Parse(_settings.AccentColor),
                HorizontalAlignment = HorizontalAlignment.Center,
                VerticalAlignment = VerticalAlignment.Center
            }
        };
        panel.Children.Add(preview);
        panel.Children.Add(CreateActionButton("Импортировать PNG-скин", async () =>
        {
            var files = await StorageProvider.OpenFilePickerAsync(new Avalonia.Platform.Storage.FilePickerOpenOptions
            {
                Title = "Выбери Minecraft-скин PNG",
                AllowMultiple = false,
                FileTypeFilter = [new Avalonia.Platform.Storage.FilePickerFileType("Изображение PNG") { Patterns = ["*.png"] }]
            });
            var path = GetLocalPath(files.FirstOrDefault());
            if (path is null)
            {
                return;
            }
            var bitmap = new Avalonia.Media.Imaging.Bitmap(path);
            if (bitmap.PixelSize.Width != 64 || bitmap.PixelSize.Height is not (32 or 64))
            {
                bitmap.Dispose();
                throw new InvalidDataException("Скин должен быть PNG размером 64×64 или 64×32.");
            }
            _skinBitmap?.Dispose();
            _skinBitmap = bitmap;
            _selectedSkinPath = path;
            preview.Child = new Avalonia.Controls.Image
            {
                Source = bitmap,
                Width = 160,
                Height = 160,
                Stretch = Stretch.Uniform
            };
            SetStatus("Скин загружен", Path.GetFileName(path));
        }));
        panel.Children.Add(CreateActionButton("Сохранить копию скина", async () =>
        {
            if (_selectedSkinPath is null || !File.Exists(_selectedSkinPath))
            {
                throw new InvalidOperationException("Сначала выбери PNG-скин.");
            }
            var target = await StorageProvider.SaveFilePickerAsync(new Avalonia.Platform.Storage.FilePickerSaveOptions
            {
                Title = "Сохранить PNG-скин",
                SuggestedFileName = Path.GetFileName(_selectedSkinPath),
                FileTypeChoices = [new Avalonia.Platform.Storage.FilePickerFileType("Изображение PNG") { Patterns = ["*.png"] }]
            });
            var path = GetLocalPath(target);
            if (path is not null)
            {
                File.Copy(_selectedSkinPath, path, overwrite: true);
                SetStatus("Скин сохранён", path);
            }
        }));
        return panel;
    }

    private Control BuildBedrock()
    {
        var panel = CreatePage("Minecraft Bedrock", "Запуск установленной официальной версии Minecraft for Windows.");
        panel.Children.Add(CreateActionButton("Запустить Minecraft Bedrock", () =>
        {
            if (!OperatingSystem.IsWindows())
            {
                throw new PlatformNotSupportedException("Запуск Bedrock доступен в Windows.");
            }
            Process.Start(new ProcessStartInfo("minecraft://") { UseShellExecute = true });
            SetStatus("Открываю Minecraft Bedrock", "Приложение запускается через Windows.");
            return Task.CompletedTask;
        }));
        panel.Children.Add(CreateActionButton("Открыть данные Bedrock", () =>
        {
            if (!OperatingSystem.IsWindows())
            {
                throw new PlatformNotSupportedException("Данные Bedrock доступны в Windows.");
            }
            var packages = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Packages");
            var bedrockPackage = Directory.Exists(packages)
                ? Directory.EnumerateDirectories(packages, "Microsoft.MinecraftUWP_*").FirstOrDefault()
                : null;
            if (bedrockPackage is null)
            {
                throw new DirectoryNotFoundException("Minecraft Bedrock не установлен для текущего пользователя.");
            }
            return OpenFolderAsync(Path.Combine(bedrockPackage, "LocalState", "games", "com.mojang"));
        }));
        return panel;
    }

    private Control BuildAccounts()
    {
        var panel = CreatePage("Вход в Microsoft", "Токен обновления хранится в Windows Credential Manager; данные аккаунта не записываются в журнал.");
        panel.Children.Add(new TextBlock { Text = $"Текущий профиль: {_settings.AccountName}", FontSize = 18, FontWeight = FontWeight.Bold });
        panel.Children.Add(CreateActionButton("Войти в Microsoft и подтвердить владение игрой", async () =>
        {
            var account = await _authentication.AuthenticateAsync(prompt =>
            {
                Dispatcher.UIThread.Post(() =>
                {
                    SetStatus($"Код входа: {prompt.UserCode}", prompt.VerificationUri);
                    Process.Start(new ProcessStartInfo(prompt.VerificationUri) { UseShellExecute = true });
                });
            });
            _settings.AccountId = account.Id;
            _settings.AccountName = account.Username;
            _settings.Username = account.Username;
            await SaveSettingsAsync();
            AccountBadge.Text = $"MICROSOFT • {account.Username}";
            SetStatus("Вход выполнен", "Владение Minecraft: Java Edition подтверждено.");
            ShowPage("accounts");
        }));
        var offlineName = CreateTextBox("Имя офлайн-профиля (3–16 символов)");
        offlineName.Text = _settings.AccountName == "Player" ? string.Empty : _settings.AccountName;
        panel.Children.Add(offlineName);
        panel.Children.Add(CreateActionButton("Использовать офлайн-профиль", async () =>
        {
            var name = offlineName.Text?.Trim() ?? string.Empty;
            if (name.Length is < 3 or > 16 || !name.All(char.IsAsciiLetterOrDigit) ||
                char.IsDigit(name[0]))
            {
                throw new ArgumentException("Офлайн-имя должно содержать 3–16 латинских букв или цифр и начинаться с буквы.");
            }
            await _authentication.SignOutAsync();
            _settings.AccountId = string.Empty;
            _settings.AccountName = name;
            _settings.Username = name;
            await SaveSettingsAsync();
            AccountBadge.Text = $"ОФЛАЙН • {name}";
            SetStatus("Офлайн-профиль готов", "Онлайн-серверы могут требовать вход Microsoft.");
        }));
        if (!string.IsNullOrWhiteSpace(_settings.AccountId))
        {
            panel.Children.Add(CreateActionButton("Выйти из аккаунта", async () =>
            {
                await _authentication.SignOutAsync();
                _settings.AccountId = string.Empty;
                _settings.AccountName = "Player";
                _settings.Username = "Player";
                await SaveSettingsAsync();
                AccountBadge.Text = "ОФЛАЙН • Player";
                ShowPage("accounts");
            }));
        }
        return panel;
    }

    private Control BuildDiagnostics()
    {
        var panel = CreatePage("Диагностика и Crash Reporter", "Проверка системы, последних логов и отчётов о сбоях.");
        panel.Children.Add(CreateActionButton("Проверить систему", async () =>
        {
            var diagnostics = await _tools.CollectDiagnosticsAsync(
                _tools.ResolveGameDirectory(_settings), _settings.JavaPath);
            var report = string.Join(Environment.NewLine,
            [
                $"ОС: {diagnostics.OperatingSystem}",
                $"Архитектура: {diagnostics.Architecture}",
                $"Потоки CPU: {diagnostics.ProcessorCount}",
                $"Доступная память: {diagnostics.TotalMemoryMb:N0} МБ",
                $"Java: {diagnostics.JavaPath}",
                $"Последний crash-report: {diagnostics.LatestCrashReport ?? "не найден"}",
                $"Последний лог: {diagnostics.LatestLogPath ?? "не найден"}",
                string.Empty,
                .. diagnostics.Warnings
            ]);
            FooterDetails.Text = report;
            SetStatus("Диагностика завершена", diagnostics.Warnings.Count == 0 ? "Критичных проблем не найдено." : $"{diagnostics.Warnings.Count} рекомендаций");
        }));
        panel.Children.Add(CreateActionButton("Открыть crash-reports", () =>
            OpenFolderAsync(Path.Combine(_tools.ResolveGameDirectory(_settings), "crash-reports"))));
        panel.Children.Add(CreateActionButton("Открыть latest.log", () =>
        {
            var logPath = Path.Combine(_tools.ResolveGameDirectory(_settings), "logs", "latest.log");
            if (!File.Exists(logPath))
            {
                throw new FileNotFoundException("Файл latest.log пока не создан.");
            }
            Process.Start(new ProcessStartInfo(logPath) { UseShellExecute = true });
            return Task.CompletedTask;
        }));
        return panel;
    }

    private Control BuildSettings()
    {
        var panel = CreatePage("Настройки", "Персонализируй внешний вид и параметры запуска Minecraft.");
        panel.Children.Add(new TextBlock
        {
            Text = "ВНЕШНИЙ ВИД",
            FontSize = 10,
            FontWeight = FontWeight.Bold,
            Foreground = Brush.Parse("#92959D"),
            Margin = new Thickness(0, 6, 0, -6)
        });
        var currentAccent = AccentPalette.FirstOrDefault(option =>
            option.Color.Equals(_settings.AccentColor, StringComparison.OrdinalIgnoreCase));
        var accentChoices = AccentPalette.Select(option => option.Name).ToList();
        var accentSelection = currentAccent.Name ??
                              AccentPalette[0].Name;
        var accentPicker = CreateCombo(accentChoices, accentSelection);
        var accentColor = CreateTextBox("Свой цвет в формате #RRGGBB");
        accentColor.Text = _settings.AccentColor;
        accentPicker.SelectionChanged += (_, _) =>
        {
            if (accentPicker.SelectedItem is string selectedName)
            {
                accentColor.Text = AccentPalette.First(option => option.Name == selectedName).Color;
            }
        };
        panel.Children.Add(new TextBlock
        {
            Text = "Цвет акцента",
            FontSize = 12,
            Foreground = Brush.Parse("#C3C5CA")
        });
        panel.Children.Add(accentPicker);
        panel.Children.Add(accentColor);

        panel.Children.Add(new TextBlock
        {
            Text = "ПАРАМЕТРЫ ИГРЫ",
            FontSize = 10,
            FontWeight = FontWeight.Bold,
            Foreground = Brush.Parse("#92959D"),
            Margin = new Thickness(0, 10, 0, -6)
        });
        var memory = new NumericUpDown { Minimum = 512, Maximum = 131072, Increment = 512, Value = _settings.MaxMemoryMb };
        var javaPath = CreateTextBox("Путь к javaw.exe (пусто — автоопределение)");
        javaPath.Text = _settings.JavaPath;
        var gameDirectory = CreateTextBox("Папка Minecraft");
        gameDirectory.Text = _settings.GameDirectory;
        var animations = new CheckBox { Content = "Анимации интерфейса", IsChecked = _settings.EnableAnimations };
        var closeAfterGame = new CheckBox { Content = "Закрывать лаунчер после запуска игры", IsChecked = _settings.CloseAfterGame };
        var fullscreen = new CheckBox { Content = "Запускать игру в полноэкранном режиме", IsChecked = _settings.Fullscreen };
        var autoMemory = new CheckBox { Content = "Автоматически подбирать объём памяти", IsChecked = _settings.AutoMemory };
        panel.Children.Add(new TextBlock { Text = "ПАМЯТЬ JVM (МБ)", FontSize = 10, FontWeight = FontWeight.Bold, Foreground = Brush.Parse(_settings.AccentColor) });
        panel.Children.Add(memory);
        panel.Children.Add(gameDirectory);
        panel.Children.Add(CreateActionButton("Выбрать папку Minecraft", async () =>
        {
            var folders = await StorageProvider.OpenFolderPickerAsync(new Avalonia.Platform.Storage.FolderPickerOpenOptions
            {
                Title = "Папка данных Minecraft",
                AllowMultiple = false
            });
            var folderPath = GetLocalPath(folders.FirstOrDefault());
            if (folderPath is not null)
            {
                gameDirectory.Text = folderPath;
            }
        }));
        panel.Children.Add(javaPath);
        panel.Children.Add(CreateCombo(["G1GC", "ZGC"], _settings.GarbageCollector,
            selection => _settings.GarbageCollector = selection));
        panel.Children.Add(CreateCombo(["Стандартная графика", "Низкая графика"], _settings.GraphicsProfile == 1
                ? "Низкая графика"
                : "Стандартная графика",
            selection => _settings.GraphicsProfile = selection == "Низкая графика" ? 1 : 0));
        panel.Children.Add(autoMemory);
        panel.Children.Add(animations);
        panel.Children.Add(closeAfterGame);
        panel.Children.Add(fullscreen);
        panel.Children.Add(CreateActionButton("Сохранить настройки", async () =>
        {
            _settings.AccentColor = accentColor.Text?.Trim() ?? string.Empty;
            _settings.MaxMemoryMb = (int)(memory.Value ?? 4096);
            _settings.JavaPath = javaPath.Text?.Trim() ?? string.Empty;
            var selectedDirectory = gameDirectory.Text?.Trim();
            if (!string.IsNullOrWhiteSpace(selectedDirectory))
            {
                _settings.GameDirectory = Path.GetFullPath(selectedDirectory);
            }
            _settings.EnableAnimations = animations.IsChecked ?? true;
            _settings.CloseAfterGame = closeAfterGame.IsChecked ?? false;
            _settings.Fullscreen = fullscreen.IsChecked ?? false;
            _settings.AutoMemory = autoMemory.IsChecked ?? true;
            await SaveSettingsAsync();
            ApplyAccent();
            Classes.Set("no-animations", !_settings.EnableAnimations);
            SetStatus("Настройки сохранены", "Изменения применены.");
        }));
        panel.Children.Add(CreateActionButton("Проверить подписанное обновление", async () =>
        {
            var update = await _updates.DownloadLatestInstallerAsync("2.2.0");
            if (update.IsCurrent)
            {
                SetStatus("Обновления нет", $"Установлена актуальная версия {update.Version}.");
                return;
            }
            Process.Start(new ProcessStartInfo(update.InstallerPath) { UseShellExecute = true });
            SetStatus("Проверка подписи пройдена", $"Запускаю установщик {update.Version}.");
        }));
        return panel;
    }

    private static Button CreateButton(string label, string style) =>
        new()
        {
            Content = label,
            Classes = { style },
            MinHeight = 46,
            MinWidth = style == "primary" ? 150 : 130
        };

    private static IEnumerable<T> FindControls<T>(Control parent) where T : Control
    {
        foreach (var child in parent.GetVisualChildren())
        {
            if (child is T match)
            {
                yield return match;
            }

            if (child is Control control)
            {
                foreach (var descendant in FindControls<T>(control))
                {
                    yield return descendant;
                }
            }
        }
    }

    private void TitleBar_PointerPressed(object? sender, PointerPressedEventArgs e)
    {
        if (e.Source is Control source && (source is Button || source.FindAncestorOfType<Button>() is not null))
        {
            return;
        }

        if (e.GetCurrentPoint(this).Properties.IsLeftButtonPressed)
        {
            BeginMoveDrag(e);
        }
    }

    private void Minimize_Click(object? sender, Avalonia.Interactivity.RoutedEventArgs e) =>
        WindowState = WindowState.Minimized;

    private void Maximize_Click(object? sender, Avalonia.Interactivity.RoutedEventArgs e) =>
        WindowState = WindowState == WindowState.Maximized ? WindowState.Normal : WindowState.Maximized;

    private void Close_Click(object? sender, Avalonia.Interactivity.RoutedEventArgs e) => Close();
}
