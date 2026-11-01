# Minty Launcher

Minty Launcher — настольный лаунчер Minecraft Java Edition для Windows 10/11 x64.

## Скачать

После успешной сборки лаунчер публикует установочный исполняемый файл в GitHub Releases:

- [Скачать MintyLauncher.exe](https://github.com/ZeeinJordansky/Launcher-minecraft-AI/releases/latest/download/MintyLauncher.exe)
- [Скачать архив исходного кода](https://github.com/ZeeinJordansky/Launcher-minecraft-AI/releases/latest/download/Minty-Launcher-source.tar.xz)

## Возможности

- Установка официальных Vanilla-релизов Minecraft Java Edition.
- Загрузка client JAR, библиотек, native-библиотек и игровых ресурсов с проверкой опубликованных Mojang SHA-1.
- Автоматическая установка подходящей Java Runtime (Temurin).
- Вход Microsoft с проверкой Xbox Live, XSTS и профиля Minecraft Java Edition. Для него требуется настроить собственный OAuth Client ID.
- Офлайн-профиль для одиночной игры.
- **Skin Studio:** создание и редактирование скина 64×64, импорт и экспорт PNG; отдельный Microsoft-вход не нужен.
- **«Слабый ПК»:** сниженные графические настройки Minecraft, ограничение памяти Java до 2 ГБ, более щадящие JVM-параметры и менее частая перерисовка интерфейса.
- Выбор пользовательского каталога установки Minecraft.
- Журнал событий лаунчера и настройки Java/памяти.
- Тёмный интерфейс с мятной цветовой темой.

## Важные ограничения

Skin Studio редактирует и экспортирует PNG. Чистый Vanilla Minecraft не применяет такой файл к офлайн-профилю автоматически: чтобы видеть собственный скин в самой игре, нужен совместимый мод загрузки локальных скинов. Для онлайн-профиля скин меняется через официальный аккаунт Microsoft/Minecraft.

Режим «Слабый ПК» выставляет более лёгкие параметры графики и запуска, но не может гарантировать конкретный прирост FPS: результат зависит от процессора, видеокарты, памяти, версии игры и мира.

Первая версия поддерживает Vanilla release; Fabric, Forge и NeoForge пока не подключены. Проверка SHA-1 нужна для контроля целостности официальных файлов и не является античитом. Сборка Windows не подписана сертификатом code signing.

## Сборка из исходников

В архиве исходников находятся Rust-приложение, Java bootstrap и документация в `minty-launcher/docs/`. Для самостоятельной сборки требуются Windows 10/11 x64, Rust stable + MSVC Build Tools и JDK 21.

Из папки `minty-launcher`:

```cmd
cargo test --all-targets --target x86_64-pc-windows-msvc
cargo build --release --target x86_64-pc-windows-msvc
```

Исполняемый файл: `target/x86_64-pc-windows-msvc/release/minty-launcher.exe`.

Исходный код автоматически собирается Windows GitHub Actions. Публикация Release происходит только после успешных тестов и сборки.
