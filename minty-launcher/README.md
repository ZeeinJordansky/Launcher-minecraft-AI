# Minty Launcher

**Minty Launcher** — нативный лаунчер Minecraft для Windows 10/11 x64. Интерфейс реализован на Rust/egui, подписи и основные сценарии доступны на русском языке.

## Возможности v0.4.0

- Экран запуска: проверка обновлений лаунчера → поиск установленной Java → загрузка интерфейса.
- Главная показывает только локально установленные Java-профили с валидными metadata и клиентскими файлами.
- Установка официальных версий Minecraft Java Edition, загрузка Java Runtime требуемой версии.
- Изоляция изменяемых данных каждой сборки: `mods`, `saves`, `options.txt`, resource packs и конфигурации. Общие библиотеки и assets не дублируются.
- Каталог Modrinth: популярные проекты, поиск, фильтрация совместимых версий и установка модов.
- Первичная статическая диагностика JAR: повреждённые архивы, повторяющиеся mod ID и несовпадение типа загрузчика.
- Полноэкранный запуск Java Edition; переключение режима — стандартной клавишей F11 в игре.
- Автоматическое обнаружение Java, ручной путь к runtime, выбор G1GC / ZGC / Shenandoah.
- Skin Studio: локальный редактор PNG 64×64, импорт и экспорт.
- Режим «Слабый ПК»: консервативные настройки графики, профиль Java-памяти и резервная копия options.txt.
- Crash Reporter: разбор minecraft.log для типовых ошибок памяти, Java, загрузчиков, модов и OpenGL.
- Bedrock Stable / Preview: запуск реально установленных каналов и импорт .mcpack, .mcaddon, .mcworld.
- Разделённые настройки: основные, Minecraft, Java, производительность, оформление, аккаунт, файлы, безопасность, обновления и журнал.
- Каталог Modrinth с категориями модов, модпаков, ресурспаков и шейдеров; фильтрация по совместимости и сортировка.

## Скачать

- [Последний EXE](https://github.com/ZeeinJordansky/Launcher-minecraft-AI/releases/latest/download/minty-launcher.exe)
- [Установщик Windows](https://github.com/ZeeinJordansky/Launcher-minecraft-AI/releases/latest/download/MintyLauncher-Setup.exe)
- [Исходный код](https://github.com/ZeeinJordansky/Launcher-minecraft-AI)

## Ограничения

- CurseForge открывается из каталога через официальный сайт. Прямая установка через API требует зарегистрированного API-ключа.
- Для модпаков, ресурспаков и шейдеров каталог показывает проекты и открывает страницу источника; прямая установка в разных форматах пока не подключена.
- Bedrock Stable / Preview устанавливаются и обновляются через Microsoft Store. Minty не скачивает неподдерживаемые Appx-пакеты и не обещает произвольный откат.
- Диагностика JAR — статический скан части метаданных. Она не обнаруживает все зависимости, все конфликты и вредоносное поведение.
- Полноценный Windows Sandbox/AppContainer/restricted-token запуск Java пока не реализован. Не запускай непроверенные моды и модпаки.
- FPS-бенчмарк с точным числом FPS не реализован: результат зависит от GPU, драйвера, мира, настроек и сборки. Minty показывает аппаратную оценку, а не выдуманные значения.
- Ely.by/Mojang skin upload и плащи не подключены. Skin Studio работает локально.
- Microsoft OAuth требует собственного Public Client ID; Client Secret не нужен и не должен помещаться в настройки.
- EXE не подписан Authenticode, поэтому Windows SmartScreen может показать предупреждение.

Подробности сборки: [docs/BUILDING.md](docs/BUILDING.md).  
MIT License — см. LICENSE.


## v0.5.0

- Frameless Minty title bar with accent-colored window controls.
- Background Java checks and game process launch run without flashing console windows.
- Unified Modrinth project cards and consistent action buttons.
- Improved Skin Workshop with opacity, grid/overlay toggles, horizontal flip, local skin save and keyboard-friendly history controls.
- Ten UI language choices for navigation and main sections.
- Click the account avatar to jump directly to the profile and skin editor.
