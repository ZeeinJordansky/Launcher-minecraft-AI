# Сборка Minty Launcher на Windows

## Требования

- Windows 10/11 x64.
- Rust stable и Cargo.
- Visual Studio 2022 Build Tools с workload Desktop development with C++ (MSVC linker и Windows SDK).
- JDK 21 x64. Команда javac -version должна выполняться из cmd.

build.rs компилирует java/MintyBootstrap.java в совместимый с Java 8 bytecode, генерирует ICO из assets/minty-launcher-icon.png и встраивает его в PE-ресурсы Windows. src/main.rs использует ту же PNG-иконку в заголовке окна и на панели задач. Поэтому при сборке нужен JDK с javac, а не только JRE.

## Сборка

Из каталога minty-launcher:

cargo test --all-targets --target x86_64-pc-windows-msvc
cargo build --release --target x86_64-pc-windows-msvc

Готовый файл: target/x86_64-pc-windows-msvc/release/minty-launcher.exe.

## Автоматическая сборка

Корневой .github/workflows/ci.yml выполняет Windows x64 build и unit-тесты при каждом push в main и pull request.

Корневой .github/workflows/release.yml выполняет отдельную сборку на Windows runner и публикует единственный бинарник minty-launcher.exe в GitHub Releases. До публикации скрипт проверяет DOS/PE-заголовки, архитектуру x64 и наличие встроенного RT_GROUP_ICON-ресурса. Релиз публикуется только после успешных тестов, компиляции и проверки PE-файла.
