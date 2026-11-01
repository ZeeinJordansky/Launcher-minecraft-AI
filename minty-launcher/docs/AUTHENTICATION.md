# Настройка входа через Microsoft

Minty Launcher использует OAuth 2.0 Device Code Flow. Для официального профиля нужны Microsoft account, активный профиль Minecraft Java Edition и публичный Client ID приложения.

## Создание приложения

1. Открой [Microsoft Entra admin center](https://entra.microsoft.com/) и создай App registration.
2. Выбери поддерживаемые типы аккаунтов, совместимые с личными Microsoft accounts (обычно **Personal Microsoft accounts only** или вариант, включающий их).
3. Скопируй **Application (client) ID** и вставь его в Minty Launcher → Настройки → Microsoft OAuth Client ID.
4. В настройках Authentication включи поддержку public client / mobile & desktop и разреши Device Code Flow (название переключателя может отличаться в актуальном интерфейсе Entra).
5. Добавь делегированные разрешения, необходимые для Xbox sign-in: `XboxLive.signin` и `offline_access`, если они доступны для выбранной конфигурации.
6. Сохрани настройки и нажми «Войти» на главной странице.

Приложение должно быть public client. Не создавай Client Secret и не помещай секреты в исходный код лаунчера: секрет, встроенный в desktop-приложение, нельзя считать конфиденциальным.

## Вход

Лаунчер получает одноразовый код, открывает страницу Microsoft, ожидает подтверждение, затем выполняет обмен Microsoft → Xbox Live → XSTS → Minecraft Services. После входа проверяются entitlement и профиль Minecraft Java Edition.

Refresh token хранится через системное хранилище учётных данных. Access token остаётся в памяти процесса и не помещается в JSON-настройки.

## Офлайн-режим

Офлайн-режим создаёт локальную игровую сессию с указанным ником и предназначен только для одиночной игры/локальных сценариев. Он не подтверждает владение Minecraft, не входит в Microsoft и не должен использоваться для обхода серверной авторизации.
