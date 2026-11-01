# Minty Launcher

Нативный лаунчер Minecraft Java Edition для Windows x64. Интерфейс и управление установкой реализованы на Rust (`egui`); запуск JVM передан небольшому Java bootstrap.

## Исходный код

[Скачать полный исходный проект](Minty-Launcher-source.zip) — архив содержит Rust-код, Java bootstrap, документацию и конфигурацию сборки.

После распаковки открой каталог `minty-launcher`.

## Возможности первой версии

- Загрузка официального манифеста и выбор релиза Minecraft Java Edition.
- Загрузка client JAR, библиотек, native-библиотек и игровых ресурсов.
- Проверка SHA-1 там, где Mojang публикует контрольную сумму.
- Подготовка classpath, JVM-аргументов и native-библиотек.
- Поиск Java и установка Temurin JRE подходящей версии.
- Microsoft Device Code Flow с проверкой Xbox Live, XSTS, Minecraft Services и профиля.
- Хранение refresh token через системное хранилище учётных данных.
- Офлайн-профиль для одиночной игры.

## Сборка на Windows

Необходимы Windows 10/11 x64, Rust stable с MSVC toolchain, Visual Studio Build Tools с C++ workload и JDK 21 (команда `javac` должна быть доступна в `PATH`).

Открой терминал в каталоге `minty-launcher` и выполни:

```cmd
cargo build --release --target x86_64-pc-windows-msvc
```

Исполняемый файл появится здесь:

`target/x86_64-pc-windows-msvc/release/minty-launcher.exe`

Подробности описаны в `docs/BUILDING.md`.

## Автоматическая сборка и Releases

- `.github/workflows/ci.yml` собирает Windows x64-версию при push в `main` и pull request. Результат сборки сохраняется как GitHub Actions artifact.
- `.github/workflows/release.yml` собирает `.exe` и прикрепляет его к GitHub Release при отправке тега `v*`.

Для публикации релиза после проверки CI:

```cmd
git tag v0.1.0
git push origin v0.1.0
```

[Открыть GitHub Releases](https://github.com/ZeeinJordansky/Launcher-minecraft-AI/releases).

## Ограничения

- Первая версия рассчитана на Vanilla release. Fabric, Forge и NeoForge пока не подключены.
- Проверка SHA-1 защищает целостность поддерживаемых официальных загрузок, но не является античитом и не гарантирует обнаружение всех сторонних модификаций.
- Для сетевой авторизации необходим собственный Microsoft OAuth Client ID.
- `.exe` не будет подписан цифровым сертификатом, если отдельно не настроить code signing.

## Лицензия

MIT.