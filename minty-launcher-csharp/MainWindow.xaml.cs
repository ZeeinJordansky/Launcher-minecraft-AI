using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media.Animation;
using Microsoft.Win32;
using MintyLauncher.Models;
using MintyLauncher.Services;

namespace MintyLauncher;

/// <summary>
/// Interaction logic for MainWindow.xaml
/// </summary>
public partial class MainWindow : Window
{
    private readonly EngineClient _engine = new();
    private readonly List<EngineVersion> _versions = [];
    private LauncherSettings _settings = new();
    private CancellationTokenSource? _operationCancellation;
    private string _currentPage = "home";
    private bool _isBusy;

    public MainWindow()
    {
        InitializeComponent();
        VersionPicker.DisplayMemberPath = nameof(EngineVersion.Id);
        VersionPicker.SelectedValuePath = nameof(EngineVersion.Id);
    }

    private async void Window_Loaded(object sender, RoutedEventArgs e)
    {
        try
        {
            var settingsEvent = await _engine.ExecuteAsync("get_settings");
            _settings = settingsEvent?.Settings ??
                throw new InvalidOperationException("Go-движок не вернул настройки лаунчера.");
            ApplySettingsToControls();
            ProfileName.Text = _settings.Username;
            FooterStatus.Text = "Подключено к Go-движку Minty";
            FooterDetails.Text = "MINTY LAUNCHER 2.0.0-GLOBAL  ·  JAVA EDITION";
            await RefreshVersionsAsync();
        }
        catch (Exception exception)
        {
            SetStatus("Движок не запущен", exception.Message, isError: true);
        }
    }

    private void ApplySettingsToControls()
    {
        GameDirectoryBox.Text = _settings.GameDirectory;
        UsernameBox.Text = _settings.Username;
        JavaPathBox.Text = _settings.JavaPath;
        MemorySlider.Value = Math.Clamp(_settings.MaxMemoryMb, 1024, 32768);
        WeakPcCheck.IsChecked = _settings.WeakPcMode;
        AnimationsCheck.IsChecked = _settings.EnableAnimations;
        IsolatedProfilesCheck.IsChecked = _settings.IsolatedProfiles;
        ModLoaderPicker.SelectedItem = ModLoaderPicker.Items.OfType<ComboBoxItem>()
            .FirstOrDefault(item => string.Equals(item.Content?.ToString(), _settings.ModLoader, StringComparison.OrdinalIgnoreCase));
    }

    private void TitleBar_OnMouseLeftButtonDown(object sender, MouseButtonEventArgs e)
    {
        if (e.ClickCount == 2)
        {
            ToggleMaximize();
            return;
        }
        if (e.ButtonState == MouseButtonState.Pressed)
        {
            DragMove();
        }
    }

    private void Minimize_Click(object sender, RoutedEventArgs e) => WindowState = WindowState.Minimized;

    private void Maximize_Click(object sender, RoutedEventArgs e) => ToggleMaximize();

    private void ToggleMaximize() => WindowState = WindowState == WindowState.Maximized
        ? WindowState.Normal
        : WindowState.Maximized;

    private void Close_Click(object sender, RoutedEventArgs e)
    {
        _operationCancellation?.Cancel();
        Close();
    }

    private void HomeNav_Click(object sender, RoutedEventArgs e) => Navigate("home");

    private void VersionsNav_Click(object sender, RoutedEventArgs e) => Navigate("versions");

    private void ModsNav_Click(object sender, RoutedEventArgs e) => Navigate("mods");

    private void SettingsNav_Click(object sender, RoutedEventArgs e) => Navigate("settings");

    private void Navigate(string page)
    {
        if (_currentPage == page)
        {
            return;
        }

        _currentPage = page;
        HomePage.Visibility = page == "home" ? Visibility.Visible : Visibility.Collapsed;
        VersionsPage.Visibility = page == "versions" ? Visibility.Visible : Visibility.Collapsed;
        ModsPage.Visibility = page == "mods" ? Visibility.Visible : Visibility.Collapsed;
        SettingsPage.Visibility = page == "settings" ? Visibility.Visible : Visibility.Collapsed;
        if (AnimationsCheck.IsChecked == true)
        {
            PageRoot.BeginAnimation(OpacityProperty, new DoubleAnimation(0, 1, TimeSpan.FromMilliseconds(190)));
            PageOffset.BeginAnimation(System.Windows.Media.TranslateTransform.YProperty,
                new DoubleAnimation(8, 0, TimeSpan.FromMilliseconds(210)));
        }
        else
        {
            PageRoot.Opacity = 1;
            PageOffset.Y = 0;
        }
    }

    private async void RefreshVersions_Click(object sender, RoutedEventArgs e) => await RefreshVersionsAsync();

    private async Task RefreshVersionsAsync()
    {
        try
        {
            IsBusy = true;
            SetStatus("Загружаем каталог", "Получаем официальный манифест Mojang.");
            var response = await _engine.ExecuteAsync("list_versions");
            _versions.Clear();
            _versions.AddRange(response?.Versions ?? []);
            if (_versions.Count == 0)
            {
                throw new InvalidOperationException("Mojang вернул пустой список версий.");
            }
            LatestLabel.Text = $"Последний релиз: {response!.Version}";
            UpdateVersionPickers(response.Version);
            ApplyVersionFilter();
            SetStatus("Каталог готов", $"Доступно версий Minecraft: {_versions.Count}.");
        }
        catch (Exception exception)
        {
            SetStatus("Не удалось загрузить версии", exception.Message, isError: true);
        }
        finally
        {
            IsBusy = false;
        }
    }

    private void UpdateVersionPickers(string latest)
    {
        var selected = _settings.SelectedVersion == "latest-release" ? latest : _settings.SelectedVersion;
        VersionPicker.ItemsSource = _versions;
        VersionPicker.SelectedValue = _versions.Any(version => version.Id == selected) ? selected : latest;
        if (VersionPicker.SelectedItem is EngineVersion item)
        {
            _settings.SelectedVersion = item.Id;
        }
    }

    private async void Play_Click(object sender, RoutedEventArgs e) => await RunEngineActionAsync("launch");

    private async void Install_Click(object sender, RoutedEventArgs e) => await RunEngineActionAsync("install_version");

    private void CancelOperation_Click(object sender, RoutedEventArgs e) => _operationCancellation?.Cancel();

    private async Task RunEngineActionAsync(string action)
    {
        if (IsBusy)
        {
            return;
        }

        SyncSettingsFromControls();
        try
        {
            IsBusy = true;
            DownloadPanel.Visibility = Visibility.Collapsed;
            _operationCancellation = new CancellationTokenSource();
            var progress = new Progress<EngineEvent>(HandleEngineEvent);
            await _engine.ExecuteAsync(action, _settings, progress, _operationCancellation.Token);
            if (action == "launch")
            {
                JavaStatus.Text = "Java Runtime готова";
                SetStatus("Minecraft запущен", $"Версия {_settings.SelectedVersion} работает в отдельном процессе.");
            }
            else
            {
                SetStatus("Установка завершена", $"Файлы версии {_settings.SelectedVersion} готовы.");
            }
        }
        catch (OperationCanceledException)
        {
            SetStatus("Операция отменена", "Загрузка остановлена.");
        }
        catch (Exception exception)
        {
            SetStatus("Не удалось выполнить операцию", exception.Message, isError: true);
        }
        finally
        {
            _operationCancellation?.Dispose();
            _operationCancellation = null;
            IsBusy = false;
            DownloadPanel.Visibility = Visibility.Collapsed;
        }
    }

    private void HandleEngineEvent(EngineEvent engineEvent)
    {
        switch (engineEvent.Type)
        {
            case "status":
                FooterStatus.Text = engineEvent.Message;
                if (engineEvent.Message.Contains("Java", StringComparison.OrdinalIgnoreCase))
                {
                    JavaStatus.Text = engineEvent.Message;
                }
                break;
            case "download_progress":
                DownloadPanel.Visibility = Visibility.Visible;
                DownloadFile.Text = engineEvent.Message;
                DownloadProgress.Value = Math.Clamp(engineEvent.Progress * 100, 0, 100);
                DownloadStats.Text = $"{FormatBytes(engineEvent.BytesPerSecond)}/с  ·  {FormatEta(engineEvent.EtaSeconds)}";
                FooterStatus.Text = "Загрузка файлов Minecraft";
                FooterDetails.Text = engineEvent.TotalBytes > 0
                    ? $"{FormatBytes(engineEvent.DownloadedBytes)} из {FormatBytes(engineEvent.TotalBytes)}"
                    : "Размер файла определяется…";
                break;
            case "error":
                FooterStatus.Text = engineEvent.Message;
                break;
            case "complete":
                FooterStatus.Text = engineEvent.Message;
                break;
        }
    }

    private static string FormatBytes(double bytes)
    {
        if (bytes < 1024)
        {
            return $"{bytes:0} Б";
        }
        if (bytes < 1024 * 1024)
        {
            return $"{bytes / 1024:0.0} КБ";
        }
        return $"{bytes / (1024 * 1024):0.0} МБ";
    }

    private static string FormatEta(long seconds)
    {
        if (seconds < 0)
        {
            return "расчёт…";
        }
        var time = TimeSpan.FromSeconds(seconds);
        return time.TotalHours >= 1 ? $"{(int)time.TotalHours}:{time.Minutes:00}:{time.Seconds:00}" : $"{time.Minutes:00}:{time.Seconds:00}";
    }

    private void VersionPicker_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (VersionPicker.SelectedItem is EngineVersion version)
        {
            _settings.SelectedVersion = version.Id;
        }
    }

    private void VersionSearch_TextChanged(object sender, TextChangedEventArgs e) => ApplyVersionFilter();

    private void VersionFilter_SelectionChanged(object sender, SelectionChangedEventArgs e) => ApplyVersionFilter();

    private void VersionList_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (VersionList.SelectedItem is EngineVersion version)
        {
            _settings.SelectedVersion = version.Id;
            VersionPicker.SelectedValue = version.Id;
        }
    }

    private void ApplyVersionFilter()
    {
        if (VersionList is null)
        {
            return;
        }
        var search = VersionSearch.Text.Trim();
        var filter = (VersionFilter.SelectedItem as ComboBoxItem)?.Content?.ToString() ?? "Все типы";
        var typeFilter = filter switch
        {
            "Old Beta" => "old_beta",
            "Old Alpha" => "old_alpha",
            _ => filter.ToLowerInvariant()
        };
        var visible = _versions.Where(version =>
            (search.Length == 0 || version.Id.Contains(search, StringComparison.OrdinalIgnoreCase)) &&
            (filter == "Все типы" || version.Type.Equals(typeFilter, StringComparison.OrdinalIgnoreCase)));
        VersionList.ItemsSource = visible.ToList();
    }

    private async void ModsSearch_Click(object sender, RoutedEventArgs e) => await SearchModsAsync();

    private async Task SearchModsAsync()
    {
        if (IsBusy)
        {
            return;
        }

        try
        {
            SyncSettingsFromControls();
            IsBusy = true;
            _operationCancellation = new CancellationTokenSource();
            ModsStatus.Text = "Ищем проекты на Modrinth…";
            var category = (ModsCategory.SelectedItem as ComboBoxItem)?.Tag?.ToString() ?? "mod";
            var sort = (ModsSort.SelectedItem as ComboBoxItem)?.Tag?.ToString() ?? "downloads";
            var result = await _engine.SearchModsAsync(
                ModsSearch.Text.Trim(), category, sort, _settings,
                cancellationToken: _operationCancellation.Token);
            ModsList.ItemsSource = result?.Projects ?? [];
            ModsStatus.Text = ModsList.Items.Count == 0
                ? "Ничего не найдено. Измени запрос или фильтры."
                : $"Найдено проектов: {ModsList.Items.Count}";
        }
        catch (OperationCanceledException)
        {
            ModsStatus.Text = "Поиск отменён.";
        }
        catch (Exception exception)
        {
            ModsStatus.Text = $"Не удалось выполнить поиск: {exception.Message}";
        }
        finally
        {
            _operationCancellation?.Dispose();
            _operationCancellation = null;
            IsBusy = false;
        }
    }

    private async void ModsSearch_KeyDown(object sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter)
        {
            e.Handled = true;
            await SearchModsAsync();
        }
    }

    private async void ModInstall_Click(object sender, RoutedEventArgs e)
    {
        if (IsBusy)
        {
            return;
        }

        if (sender is not Button { DataContext: EngineProject project })
        {
            return;
        }
        var slug = project.Slug;
        var category = project.ProjectType;
        if (category == "modpack")
        {
            try
            {
                Process.Start(new ProcessStartInfo($"https://modrinth.com/modpack/{Uri.EscapeDataString(slug)}") { UseShellExecute = true });
            }
            catch (Exception exception)
            {
                SetStatus("Не удалось открыть Modrinth", exception.Message, isError: true);
            }
            return;
        }
        SyncSettingsFromControls();
        try
        {
            IsBusy = true;
            DownloadPanel.Visibility = Visibility.Collapsed;
            _operationCancellation = new CancellationTokenSource();
            var progress = new Progress<EngineEvent>(HandleEngineEvent);
            var result = await _engine.InstallModAsync(slug, category, _settings, progress, _operationCancellation.Token);
            SetStatus("Контент установлен", result?.Message ?? slug);
        }
        catch (OperationCanceledException)
        {
            SetStatus("Установка отменена", "Загрузка контента остановлена.");
        }
        catch (Exception exception)
        {
            SetStatus("Не удалось установить проект", exception.Message, isError: true);
        }
        finally
        {
            _operationCancellation?.Dispose();
            _operationCancellation = null;
            IsBusy = false;
            DownloadPanel.Visibility = Visibility.Collapsed;
        }
    }

    private void BrowseGameDirectory_Click(object sender, RoutedEventArgs e)
    {
        var dialog = new OpenFolderDialog
        {
            Title = "Выберите папку Minecraft",
            InitialDirectory = Directory.Exists(GameDirectoryBox.Text) ? GameDirectoryBox.Text : Environment.GetFolderPath(Environment.SpecialFolder.UserProfile)
        };
        if (dialog.ShowDialog(this) == true)
        {
            GameDirectoryBox.Text = dialog.FolderName;
        }
    }

    private void BrowseJava_Click(object sender, RoutedEventArgs e)
    {
        var dialog = new OpenFileDialog
        {
            Title = "Выберите исполняемый файл Java",
            Filter = "Java executable (java.exe;javaw.exe)|java.exe;javaw.exe|All files (*.*)|*.*",
            CheckFileExists = true
        };
        if (dialog.ShowDialog(this) == true)
        {
            JavaPathBox.Text = dialog.FileName;
        }
    }

    private void OpenModsFolder_Click(object sender, RoutedEventArgs e)
    {
        try
        {
            SyncSettingsFromControls();
            if (!Path.IsPathFullyQualified(_settings.GameDirectory))
            {
                throw new InvalidOperationException("Папка Minecraft должна быть указана абсолютным путём.");
            }
            var profileDirectory = _settings.IsolatedProfiles
                ? Path.Combine(_settings.GameDirectory, "instances", _settings.SelectedVersion)
                : _settings.GameDirectory;
            var modsDirectory = Path.Combine(profileDirectory, "mods");
            Directory.CreateDirectory(modsDirectory);
            Process.Start(new ProcessStartInfo(modsDirectory) { UseShellExecute = true });
        }
        catch (Exception exception)
        {
            SetStatus("Не удалось открыть папку модов", exception.Message, isError: true);
        }
    }

    private void MemorySlider_ValueChanged(object sender, RoutedPropertyChangedEventArgs<double> e)
    {
        if (MemoryLabel is not null)
        {
            MemoryLabel.Text = $"{(int)e.NewValue} MB";
        }
    }

    private async void SaveSettings_Click(object sender, RoutedEventArgs e)
    {
        try
        {
            SyncSettingsFromControls();
            IsBusy = true;
            await _engine.ExecuteAsync("save_settings", _settings);
            ProfileName.Text = _settings.Username;
            SettingsStatus.Text = "Сохранено";
            FooterStatus.Text = "Настройки сохранены";
        }
        catch (Exception exception)
        {
            SettingsStatus.Text = exception.Message;
            SetStatus("Проверь настройки", exception.Message, isError: true);
        }
        finally
        {
            IsBusy = false;
        }
    }

    private void SyncSettingsFromControls()
    {
        _settings.GameDirectory = GameDirectoryBox.Text.Trim();
        _settings.Username = UsernameBox.Text.Trim();
        _settings.JavaPath = JavaPathBox.Text.Trim();
        _settings.MaxMemoryMb = (int)MemorySlider.Value;
        _settings.WeakPcMode = WeakPcCheck.IsChecked == true;
        _settings.EnableAnimations = AnimationsCheck.IsChecked == true;
        _settings.IsolatedProfiles = IsolatedProfilesCheck.IsChecked == true;
        _settings.ModLoader = (ModLoaderPicker.SelectedItem as ComboBoxItem)?.Content?.ToString() ?? "Vanilla";
    }

    private void SetStatus(string headline, string description, bool isError = false)
    {
        StatusHeadline.Text = headline;
        StatusDescription.Text = description;
        FooterStatus.Text = headline;
        FooterDetails.Text = description;
        StatusHeadline.Foreground = isError ? System.Windows.Media.Brushes.Salmon : FindResource("TextPrimary") as System.Windows.Media.Brush;
    }

    private bool IsBusy
    {
        set
        {
            _isBusy = value;
            PlayButton.IsEnabled = !value;
            InstallButton.IsEnabled = !value;
            ModsSearchButton.IsEnabled = !value;
            ModsList.IsEnabled = !value;
        }
        get => _isBusy;
    }
}