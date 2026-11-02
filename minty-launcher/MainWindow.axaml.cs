using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Input;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Platform;
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
    private readonly Avalonia.Media.Imaging.Bitmap _launcherAvatar =
        new(AssetLoader.Open(new Uri("avares://minty-launcher/assets/minty-launcher-icon.png")));

    private static readonly string[] Navigation =
    [
        "home", "instances", "versions", "mods", "worlds", "skin",
        "bedrock", "accounts", "diagnostics", "settings"
    ];

    private static readonly string[] PageNames =
    [
        "Дом там, где твой мир", "Твои сборки", "Версии Minecraft", "Каталог контента",
        "Миры и ресурсы", "Skin Studio", "Bedrock Edition", "Аккаунты",
        "Диагностика", "Настройки"
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
            _launcherAvatar.Dispose();
        };
        Opened += async (_, _) =>
        {
            try
            {
                _settings = await _settingsStore.LoadAsync();
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
    }

    private Control BuildHome()
    {
        var root = new StackPanel { Spacing = 20 };
        var heroLayout = new Grid { ColumnDefinitions = new ColumnDefinitions("*,Auto") };
        heroLayout.Children.Add(new StackPanel
        {
            VerticalAlignment = VerticalAlignment.Center,
            Spacing = 13,
            Children =
            {
                new Border
                {
                    CornerRadius = new CornerRadius(12),
                    Padding = new Thickness(12, 7),
                    Background = Brush.Parse("#274336"),
                    HorizontalAlignment = HorizontalAlignment.Left,
                    Child = new TextBlock
                    {
                        Text = "YOUR NEXT ADVENTURE",
                        FontSize = 9,
                        FontWeight = FontWeight.Bold,
                        Foreground = Brush.Parse("#B4E9CA"),
                        LetterSpacing = 1.5
                    }
                },
                new TextBlock
                {
                    Text = "Мир начинается\nс одного клика.",
                    FontSize = 38,
                    FontWeight = FontWeight.Black,
                    LineHeight = 44
                },
                new TextBlock
                {
                    Text = "Всё, что нужно для твоего Minecraft —\nсобрано в одном уютном месте.",
                    FontSize = 14,
                    Foreground = Brush.Parse("#B6C7BC"),
                    LineHeight = 22
                },
                new StackPanel
                {
                    Orientation = Orientation.Horizontal,
                    Spacing = 10,
                    Margin = new Thickness(0, 7, 0, 0),
                    Children =
                    {
                        CreatePlayButton(),
                        CreatePageButton("＋   Новая сборка", "secondary", "instances")
                    }
                }
            }
        });
        var avatar = new Border
        {
            Width = 210,
            Height = 210,
            CornerRadius = new CornerRadius(105),
            Background = Brush.Parse("#2B4938"),
            BorderBrush = Brush.Parse("#557C62"),
            BorderThickness = new Thickness(1),
            VerticalAlignment = VerticalAlignment.Center,
            Child = new Image
            {
                Source = _launcherAvatar,
                Width = 190,
                Height = 190,
                Stretch = Stretch.Uniform
            }
        };
        Grid.SetColumn(avatar, 1);
        heroLayout.Children.Add(avatar);

        var hero = new Border
        {
            MinHeight = 290,
            CornerRadius = new CornerRadius(26),
            Padding = new Thickness(34),
            Background = new LinearGradientBrush
            {
                StartPoint = new RelativePoint(0, 0, RelativeUnit.Relative),
                EndPoint = new RelativePoint(1, 1, RelativeUnit.Relative),
                GradientStops =
                [
                    new GradientStop(Color.Parse("#22392D"), 0),
                    new GradientStop(Color.Parse("#17251E"), .58),
                    new GradientStop(Color.Parse("#122019"), 1)
                ]
            },
            BorderBrush = Brush.Parse("#3D5A48"),
            BorderThickness = new Thickness(1),
            Child = heroLayout
        };
        root.Children.Add(hero);
        root.Children.Add(new TextBlock
        {
            Text = "ТВОЯ ИГРА — ТВОИ ПРАВИЛА",
            FontSize = 10,
            FontWeight = FontWeight.Bold,
            Foreground = Brush.Parse("#91A398"),
            LetterSpacing = 1.5,
            Margin = new Thickness(2, 4, 0, 0)
        });

        var cards = new UniformGrid { Columns = 3 };
        cards.Children.Add(CreateInfoCard("01", "Версии и сборки", "Vanilla, Fabric, Forge и отдельные профили.", "Открыть библиотеку", "instances"));
        cards.Children.Add(CreateInfoCard("02", "Каталог Modrinth", "Моды, ресурспаки и шейдеры с проверкой файлов.", "Исследовать", "mods"));
        cards.Children.Add(CreateInfoCard("03", "Твой профиль", "Аккаунт, скин, Java и производительность.", "Персонализировать", "accounts"));
        root.Children.Add(cards);
        return root;
    }

    private Control BuildInstances()
    {
        var panel = CreatePage("Независимые игровые сборки", "У каждой сборки — отдельная папка, версия и набор модов.");
        var profilePicker = CreateCombo(_tools.GetProfiles(_settings), _settings.SelectedProfileId);
        profilePicker.SelectionChanged += async (_, _) =>
        {
            if (profilePicker.SelectedItem is string profile)
            {
                _settings.SelectedProfileId = profile;
                await SaveSettingsAsync();
                ShowPage("instances");
            }
        };
        panel.Children.Add(profilePicker);
        var newProfile = CreateTextBox("Название новой сборки");
        panel.Children.Add(newProfile);
        panel.Children.Add(CreateActionButton("Создать изолированную сборку", async () =>
        {
            _settings.IsolatedProfiles = true;
            _tools.AddProfile(_settings, newProfile.Text ?? string.Empty);
            await SaveSettingsAsync();
            ShowPage("instances");
            SetStatus("Сборка создана", _tools.ResolveGameDirectory(_settings));
        }));
        panel.Children.Add(CreateCombo(["Vanilla", "Fabric", "Quilt", "Forge", "NeoForge"], _settings.ModLoader,
            selection =>
            {
                _settings.ModLoader = selection;
                _settings.SelectedLoaderProfileId = string.Empty;
            }));
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
        var isolatedProfiles = new CheckBox
        {
            Content = "Изолировать версии и моды по сборкам",
            IsChecked = _settings.IsolatedProfiles
        };
        isolatedProfiles.Click += async (_, _) =>
        {
            _settings.IsolatedProfiles = isolatedProfiles.IsChecked ?? false;
            await SaveSettingsAsync();
        };
        panel.Children.Add(isolatedProfiles);
        return panel;
    }

    private StackPanel CreatePage(string title, string description)
    {
        var panel = new StackPanel { Spacing = 14, MaxWidth = 900 };
        panel.Children.Add(new TextBlock { Text = title, FontSize = 30, FontWeight = FontWeight.Bold });
        panel.Children.Add(new TextBlock
        {
            Text = description,
            FontSize = 13,
            Foreground = Brush.Parse("#A4B3AA"),
            TextWrapping = TextWrapping.Wrap,
            MaxWidth = 760
        });
        return panel;
    }

    private Button CreatePlayButton()
    {
        var button = CreateButton("▶   Играть", "primary");
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
            Background = Brush.Parse("#101915"),
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
        await _settingsStore.SaveAsync(_settings);
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
        var panel = CreatePage("Официальный каталог Minecraft", "Стабильные релизы и снапшоты из каталога Mojang.");
        var selectedVersion = CreateTextBox(_settings.SelectedVersion);
        var versionResults = new StackPanel { Spacing = 6 };
        selectedVersion.Text = _settings.SelectedVersion;
        panel.Children.Add(selectedVersion);
        panel.Children.Add(CreateActionButton("Загрузить версии", async () =>
        {
            var versions = await _minecraft.GetVersionsAsync();
            versionResults.Children.Clear();
            foreach (var item in versions.Take(80))
            {
                versionResults.Children.Add(CreatePageButton(
                    $"{item.Id}   ·   {item.Type}",
                    item.Id == _settings.SelectedVersion ? "primary" : "secondary",
                    "versions",
                    async () =>
                    {
                        _settings.SelectedVersion = item.Id;
                        selectedVersion.Text = item.Id;
                        await SaveSettingsAsync();
                        SetStatus("Версия выбрана", item.Id);
                    }));
            }
            SetStatus("Каталог загружен", $"{versions.Count} версий");
        }));
        panel.Children.Add(versionResults);
        panel.Children.Add(CreateActionButton("Выбрать введённую версию", async () =>
        {
            _settings.SelectedVersion = selectedVersion.Text?.Trim() ?? "latest-release";
            await SaveSettingsAsync();
            SetStatus("Версия выбрана", _settings.SelectedVersion);
        }));
        return panel;
    }

    private Control BuildMods()
    {
        var panel = CreatePage("Моды для твоей сборки", "Поиск Modrinth с проверкой SHA-1/SHA-512 перед установкой.");
        var query = CreateTextBox("Найти мод");
        var contentType = CreateCombo(
            ["Моды", "Ресурспаки", "Шейдеры"],
            "Моды");
        var results = new StackPanel { Spacing = 8 };
        panel.Children.Add(query);
        panel.Children.Add(contentType);
        panel.Children.Add(CreateActionButton("Искать моды", async () =>
        {
            results.Children.Clear();
            var projectType = contentType.SelectedItem switch
            {
                "Ресурспаки" => "resourcepack",
                "Шейдеры" => "shader",
                _ => "mod"
            };
            var projects = await _modrinth.SearchAsync(
                query.Text ?? string.Empty,
                "all",
                await ResolveSelectedVersionAsync(),
                _settings.ModLoader,
                projectType);
            foreach (var project in projects)
            {
                var row = new Border
                {
                    Padding = new Thickness(14),
                    CornerRadius = new CornerRadius(14),
                    Background = Brush.Parse("#121B17"),
                    BorderBrush = Brush.Parse("#293830"),
                    BorderThickness = new Thickness(1),
                    Child = new StackPanel
                    {
                        Spacing = 7,
                        Children =
                        {
                            new TextBlock { Text = project.Title, FontSize = 16, FontWeight = FontWeight.Bold },
                            new TextBlock
                            {
                                Text = $"{project.Description}   ·   {project.Downloads:N0} загрузок",
                                FontSize = 11,
                                Foreground = Brush.Parse("#A4B3AA"),
                                TextWrapping = TextWrapping.Wrap
                            },
                            CreateActionButton("Установить", async () =>
                            {
                                var path = await _modrinth.InstallAsync(
                                    project.Id,
                                    _tools.ResolveGameDirectory(_settings),
                                    await ResolveSelectedVersionAsync(),
                                    _settings.ModLoader,
                                    projectType);
                                SetStatus("Мод установлен", Path.GetFileName(path));
                            })
                        }
                    }
                };
                results.Children.Add(row);
            }
            SetStatus("Поиск завершён", $"Найдено: {projects.Count}");
        }));
        panel.Children.Add(results);

        var installedMods = new StackPanel { Spacing = 8 };
        panel.Children.Add(new TextBlock { Text = "УСТАНОВЛЕННЫЕ МОДЫ", FontSize = 10, FontWeight = FontWeight.Bold, Foreground = Brush.Parse("#A7E8C6") });
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
        var panel = CreatePage("Skin Studio", "Импортируй PNG-скин и сохрани его в своей библиотеке.");
        var preview = new Border
        {
            Width = 180,
            Height = 180,
            CornerRadius = new CornerRadius(24),
            Background = Brush.Parse("#20382B"),
            Child = new TextBlock
            {
                Text = "M",
                FontSize = 110,
                FontWeight = FontWeight.Black,
                Foreground = Brush.Parse("#A7E8C6"),
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
        var panel = CreatePage("Настройки лаунчера", "Управляй Java, памятью, графикой и поведением игры.");
        var memory = new NumericUpDown { Minimum = 512, Maximum = 131072, Increment = 512, Value = _settings.MaxMemoryMb };
        var javaPath = CreateTextBox("Путь к javaw.exe (пусто — автоопределение)");
        javaPath.Text = _settings.JavaPath;
        var gameDirectory = CreateTextBox("Папка Minecraft");
        gameDirectory.Text = _settings.GameDirectory;
        var animations = new CheckBox { Content = "Анимации интерфейса", IsChecked = _settings.EnableAnimations };
        var closeAfterGame = new CheckBox { Content = "Закрывать лаунчер после запуска игры", IsChecked = _settings.CloseAfterGame };
        var fullscreen = new CheckBox { Content = "Запускать игру в полноэкранном режиме", IsChecked = _settings.Fullscreen };
        var autoMemory = new CheckBox { Content = "Автоматически подбирать объём памяти", IsChecked = _settings.AutoMemory };
        panel.Children.Add(new TextBlock { Text = "ПАМЯТЬ JVM (МБ)", FontSize = 10, FontWeight = FontWeight.Bold, Foreground = Brush.Parse("#A7E8C6") });
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
            Classes.Set("no-animations", !_settings.EnableAnimations);
            SetStatus("Настройки сохранены", "Изменения применены.");
        }));
        panel.Children.Add(CreateActionButton("Проверить подписанное обновление", async () =>
        {
            var update = await _updates.DownloadLatestInstallerAsync("2.1.0");
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

    private Control CreateInfoCard(string number, string title, string description, string action, string page)
    {
        var card = new Border
        {
            Margin = new Thickness(0, 0, 14, 0),
            Padding = new Thickness(20),
            CornerRadius = new CornerRadius(20),
            Background = Brush.Parse("#121B17"),
            BorderBrush = Brush.Parse("#293830"),
            BorderThickness = new Thickness(1)
        };
        var stack = new StackPanel { Spacing = 12 };
        stack.Children.Add(new TextBlock { Text = number, FontSize = 10, FontWeight = FontWeight.Bold, Foreground = Brush.Parse("#91C7A6") });
        stack.Children.Add(new TextBlock { Text = title, FontSize = 17, FontWeight = FontWeight.Bold });
        stack.Children.Add(new TextBlock { Text = description, FontSize = 11, Foreground = Brush.Parse("#96A79C"), TextWrapping = TextWrapping.Wrap, MinHeight = 38 });
        var button = CreateButton(action + "   →", "secondary");
        button.Tag = page;
        button.HorizontalAlignment = HorizontalAlignment.Left;
        button.Click += (_, _) => ShowPage(page);
        stack.Children.Add(button);
        card.Child = stack;
        return card;
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
