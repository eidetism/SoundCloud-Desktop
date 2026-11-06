# Прогресс по фазам

Исходное ТЗ: `C:\Users\panic\Downloads\claude-code-prompt_1.md`. План и инвентаризация: [FORK_PLAN.md](FORK_PLAN.md). Правила для агента: [../CLAUDE.md](../CLAUDE.md).
Порядок работы: после каждой фазы отчёт и ожидание «дальше». Одна фаза — один коммит, сообщения на английском, ветка `main`. Пушить по слову пользователя.

Репозиторий: https://github.com/eidetism/SoundCloud-Desktop, локально `C:\Users\panic\Documents\soundcloud-rust-client`.

## Принятые решения (Q1–Q6 из FORK_PLAN §7, все приняты)
1. Фронтенд пишется с нуля. Донорский UI удалён.
2. `desktop/` перенесён в корень.
3. Rust-запросы шлют реальный User-Agent WebView2. Он читается нативно в окне входа и хранится в `settings.json` → `browserUserAgent`.
4. Офлайн без ffmpeg: MP3 хранится как есть, HLS склеивается из сегментов.
5. Discord RPC: нужен свой Application ID. Сейчас стоит донорский `1431978756687265872` в `src-tauri/src/discord/commands.rs`, заменить в фазе 4. Попросить ID у пользователя.
6. Мини-плеер-поповер трея убран, в трее только нативное меню.
- Название продукта «SC Desktop», бинарник `scdesktop.exe`, identifier `dev.eidetism.scdesktop`.

## Фаза 0 — разведка и план ✅
Коммит `be3af96d`, запушен. Сделаны `docs/FORK_PLAN.md`, новый `CLAUDE.md`, `docs/design/mockup.html`.

## Фаза 1 — каркас ✅
Коммит `9b15f377`, запушен.
- Удалены `App/`, `bindings/`, `depens/`, `utils/`, `themes/`, CI и доки автора. `desktop/` перенесён в корень.
- В Rust остались: `audio/` (без EQ, эффектов, pitch, A-B loop и предпрослушивания), `app/` (трей, автостарт, лог с `redact()`, навигация, проверка WebView2, `locale.rs`), `discord/`.
- Удалены `wreq` и `sc-fingerprint` (эмуляция TLS-отпечатка), git-крейты автора и весь код под Linux и macOS.
- Фронт: Vite + React 19 + Tailwind 4 + i18next (ru/en по `navigator.language`) + Inter.
- Бандл только NSIS, release-сборка проходит.
- Нужные позже модули донора удалены и переносятся из коммита `c4206f91` (`git show c4206f91:desktop/src-tauri/src/...`). Расписание по фазам в FORK_PLAN §9.

## Фаза 2 — сеть ✅
Коммит `3993b1ba`, запушен.
- `src-tauri/src/net/`: единый `reqwest` 0.13 (rustls + ring, без системного прокси). Режимы Direct, SOCKS5 (`socks5h`), HTTP, HTTPS с логином. Клиент пересобирается при сохранении.
- `settings.rs`: `%APPDATA%\dev.eidetism.scdesktop\settings.json`, без секретов.
- `secrets.rs`: Windows Credential Manager. Секреты `proxy-password` и `oauth-token`.
- «Проверить соединение» (`net_check`) проверяет черновик настроек по `api-v2.soundcloud.com` и `i1.sndcdn.com`.
- Вживую проверены Direct, SOCKS5 с логином (DNS идёт через прокси) и сохранение пароля. HTTP/HTTPS-прокси вживую **не проверены**.

## Фаза 3 — авторизация ✅
Закоммичена (см. `git log`).
- `api/`: `client_id` берётся со страницы soundcloud.com (`__sc_hydration` → `apiClient`). Запрос `/me` идёт с `Authorization: OAuth`. На 401/403 `client_id` обновляется один раз, на 429/5xx запрос повторяется с backoff. Модель `Me`.
- `auth/mod.rs`: сессия, токен в Credential Manager, кэш профиля в `session.json`. На старте `/me` проверяется в фоне. Разлогин только при 401 на `/me`. `auth_logout` стирает токен, профиль WebView не трогает.
- `auth/login.rs`: окно `login` на `https://soundcloud.com/signin` с постоянным профилем `%APPDATA%\dev.eidetism.scdesktop\login-profile`.
  - Без подмены UA, без JS-инъекций и автозаполнения.
  - Раз в секунду ищется cookie `oauth_token` → проверка `/me` → вход → окно уничтожается.
  - Через нативный WebView2 (`webview2-com`) читается UA и ловится `FrameNavigationStarting` на `captcha-delivery.com`. Тогда показывается окно-плашка `login-hint` (`index.html?view=captcha-hint`).
- `auth/browser_proxy.rs`: флаги прокси для WebView2. Direct → `--no-proxy-server`, прокси без логина → `--proxy-server`.
- `auth/forwarder.rs`: локальный SOCKS5 на `127.0.0.1` для прокси с логином (SOCKS5 / HTTP CONNECT / HTTPS CONNECT).
- `app/navigation.rs`: окну `login` разрешена любая https-навигация.
- Фронт: гейт «без сессии только экран входа» (`src/auth/*`), «Привет, {имя}», «Выйти».
- Новые зависимости: `webview2-com`, `windows` (версии как у Tauri), `tokio-rustls`, `rustls-platform-verifier`, `base64`, у `reqwest` добавлена фича `query`.
- Проверки: `cargo clippy -D warnings` чисто, 56 тестов, `tsc`/`biome` чисто, release-сборка проходит.
- **Вживую (пользователь):** вход, перезапуск (сессия сохранилась), «Выйти» → «Войти» (профиль WebView переиспользован). Токен в Credential Manager.

### Открытые хвосты фазы 3
- Не проверены: вход через Google/Apple (всплывающие окна) и плашка капчи на реальной проверке DataDome (у пользователя капча не появилась).
- HTTP/HTTPS-прокси и форвардер вживую не проверены (форвардер покрыт тестами на in-memory потоках).

## Дальше
- **Фаза 4 — API + плеер.**
  - Перенести `track_cache/sc_anon` (транскодинги, HLS) и `shared/urn.rs` из `c4206f91`.
  - Очередь в Rust, воспроизведение из Rust, SMTC, Discord RPC (свой App ID), трей.
  - Прежде чем писать модели, попросить у пользователя HAR (главная, лента, лайки, история, трек с комментариями, артист, поиск) в `docs/har/`. Папка в `.gitignore`, в репо только обрезанные фикстуры без токенов.
- **Фаза 5:** UI-оболочка по `docs/design/mockup.html` + `scimg://` (перенос `network/image_cache.rs`, `shared/file_lru.rs`).
- **Фаза 7:** нужен второй макет, страница трека, его ещё нет.
- **Фаза 8:** нужен HAR сообщений.
- **Фаза 9 (риск):** SMTC (`souvlaki`) привязан к HWND главного окна. При уничтожении окна в трее понадобится отдельное скрытое окно-владелец.
- **Фаза 10:** ключ подписи апдейтов, его сгенерирует пользователь.

## Как запускать
- Окружение: `COREPACK_ENABLE_DOWNLOAD_PROMPT=0` (pnpm 12.18 через corepack).
- Фронт: `pnpm tsc --noEmit`, `pnpm biome check src/`, `pnpm build`.
- Rust (из `src-tauri/`): `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- Release без установщика: `pnpm tauri build --no-bundle`. Если `scdesktop.exe` запущен (в том числе свёрнут в трей), сборка падает с «Отказано в доступе». Попросить пользователя закрыть приложение.
- Ручная проверка UI: PowerShell UI Automation по именам элементов. Дерево WebView2 появляется после первого `FindAll` и паузы ~3 с.
- **Важно: MSIX-виртуализация.** Claude Desktop работает в MSIX-контейнере. Процессы, запущенные агентом (и его PowerShell/Bash), пишут и читают `%APPDATA%`/`%LOCALAPPDATA%` через виртуализированную копию `%LOCALAPPDATA%\Packages\Claude_pzs8sxrjxfjjc\LocalCache\{Local,Roaming}\dev.eidetism.scdesktop`, которая перекрывает настоящие файлы. Поэтому агент не видит лог, настройки и `session.json` из запусков пользователя, а пользователь не видит файлы агента. Credential Manager не виртуализируется и общий. Мнимый «баг логирования из Проводника» объясняется именно этим, бага нет.
