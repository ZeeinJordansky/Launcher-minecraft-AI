# Сборка Minty Launcher на Windows

## Требования

- Windows 10/11 x64.
- Rust stable и Cargo: https://www.rust-lang.org/tools/install
- Visual Studio 2022 Build Tools с workload **Desktop development with C++** (MSVC linker и Windows SDK).
- JDK 21 x64. Команда `javac -version` должна выполняться из `cmd`.

`build.rs` компилирует `java/MintyBootstrap.java` в совместимый с Java 8 bytecode. Поэтому при сборке необходим именно JDK с `javac`, а не только JRE.

## Сборка

Открой `cmd` в каталоге проекта и выполни:

```cmd
cargo build --release --target x86_64-pc-windows-msvc
```

Готовый файл:

```text
target_64-pc-windows-msvcelease\minty-launcher.exe
```

Для запуска в режиме разработки:

```cmd
cargo run
```

## Автоматическая сборка

`.github/workflows/ci.yml` выполняет Windows x64 build и unit-тесты при каждом push в `main` и pull request. Успешная сборка прикладывается к Actions run как artifact.

`.github/workflows/release.yml` выполняет отдельную чистую сборку на Windows runner и публикует `MintyLauncher.exe` в GitHub Releases. Тег релиза формируется из номера workflow run (`v0.1.x`). Релиз публикуется только после успешного завершения компиляции.
