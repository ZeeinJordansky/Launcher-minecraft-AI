# Архитектура Minty Launcher

## Компоненты

- `app.rs`: UI на egui, этапный splash, навигация, библиотека профилей, Skin Studio, журнал и фоновые задачи.
- `config.rs`: настройки лаунчера, журнал и вычисление папки активного игрового экземпляра.
- `crash.rs`: статический разбор последнего minecraft.log и подсказки по типовым ошибкам.
- `mods.rs`: каталог Modrinth, установка JAR и статический анализ метаданных модов.
- `minecraft.rs`: официальный манифест Mojang, client JAR, libraries, natives, assets и запуск JVM.
- `runtime.rs`: обнаружение Java и установка Temurin Runtime требуемой версии.
- `bedrock.rs`: запуск зарегистрированных URI Bedrock Stable/Preview и импорт пакетов Minecraft.

## Изоляция экземпляров

При включённой настройке изменяемые данные профиля хранятся в `<game_dir>/instances/<profile-id>/`: `mods`, `saves`, `options.txt`, resource packs и конфигурации. Общие `versions`, `libraries`, `assets` и natives остаются в корневой папке, чтобы избежать повторной загрузки крупных файлов. По умолчанию настройка выключена, чтобы сохранить старое расположение миров и модов.

## Производительность

Профиль «Слабый ПК» снижает дальность прорисовки, simulation distance, частицы, облака и некоторые эффекты. Исходный options.txt резервируется и восстанавливается при выключении профиля. G1GC используется как безопасный вариант по умолчанию; ZGC и Shenandoah требуют совместимого runtime.

## Диагностика и границы безопасности

1. Загрузки выполняются через TLS, опубликованные хеши проверяются там, где они предоставлены источником.
2. ZIP-архивы извлекаются с проверкой безопасного относительного пути.
3. Microsoft refresh token не хранится в settings.json.
4. Анализ модов читает JAR metadata и не запускает мод-код.
5. Crash Reporter формирует эвристические подсказки, а не полную экспертную атрибуцию вылета.
6. Запуск Java пока не ограничивается AppContainer/restricted token. Поэтому это не песочница для непроверенных модов.


## Skin Workshop and catalogue
Skin Workshop edits a full 64×64 RGBA UV atlas with region shortcuts, brush, eraser, flood fill, eyedropper, brush size, horizontal mirror, zoom, palette, undo/redo and local PNG import/export. The preview is a 2D front-facing UV-derived composite, not a fully rigged 3D viewer.
Modrinth search accepts a project-type facet and sort order. Only mod projects are installed as JARs; modpacks, resource packs and shaders have different import/install formats and currently open their source project page. CurseForge discovery links to its official catalogue; direct API installs require a developer key.
