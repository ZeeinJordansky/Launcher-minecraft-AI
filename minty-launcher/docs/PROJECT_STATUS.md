# Состояние проекта

## Реализовано в исходниках

- UI на Rust/egui для Windows x64.
- Установка и запуск официальных Vanilla release.
- Microsoft OAuth / Xbox Live / Minecraft Services flow.
- Offline-профиль одиночной игры.
- Skin Studio: pixel canvas, PNG import/export.
- Профиль «Слабый ПК» с бэкапом и восстановлением `options.txt`.
- Выбор game directory.
- Настройки и локальный журнал.
- Unit-тесты логики скина, имени офлайн-профиля и восстановления настроек.

## Публикация

GitHub Actions отдельно выполняет Windows x64 тесты/сборку и workflow публикации. `MintyLauncher.exe` считается выпущенным только после успешного завершения `Minty Launcher - Windows Release` и появления бинарного файла в GitHub Releases.

## Известные ограничения

- Только Vanilla release; загрузчики модов пока не входят в проект.
- Офлайн Skin Studio экспортирует PNG, но чистый Vanilla Minecraft не применяет локальную текстуру в офлайн-профиле автоматически.
- «Слабый ПК» уменьшает часть графических настроек и JVM-памяти. Прирост FPS зависит от конфигурации устройства и не гарантируется.
- На данный момент отсутствует Authenticode-подпись Windows executable.
