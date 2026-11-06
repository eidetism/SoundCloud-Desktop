# FORK_PLAN — личный SoundCloud-клиент для Windows 11

Форк `zxcloli666/SoundCloud-Desktop` (MIT) → `eidetism/SoundCloud-Desktop`.
База разведки: коммит `c4206f91` (v8.8.0), 2026-11-06.

## 1. Что в репозитории сейчас

| Путь | Что это | Решение |
|---|---|---|
| `App/` | RN-миграция автора | удалить |
| `bindings/` (`sc-rpc`) | мост RN ↔ ядро | удалить |
| `depens/` | крейты для RN-шелла: `call-agent`, `desktop-bridge`, `discord-rpc`, `media-controls`, `tray` (ksni = Linux) | удалить целиком, см. §4.2 |
| `utils/{call,decrypt,decrypt-client}` | звонки и расшифровка | удалить |
| `themes/`, `_refs/`, `scripts/` | темы, картинки README, CI-скрипты | удалить |
| `docs/*` (кроме `design/`) | доки автора (custom CSS, chat-protocol, QR и т. п.) | удалить |
| `.github/workflows/*`, `.github/scripts/*` | CI автора (AUR, flatpak, code-review…) | удалить; в фазе 10 свой `release.yml` |
| `CLAUDE.md`, `desktop/AGENTS.md`, `desktop/CLAUDE.md`, `desktop/GLASS_UI_GUIDE.md`, `*.iml` | инструкции под RN-миграцию | удалить/заменить (корневой `CLAUDE.md` уже переписан) |
| `LICENSE` | MIT, © zxcloli666 | **оставить без изменений** |
| `README.md` | маркетинг автора | переписать, первой строкой «Fork of zxcloli666/SoundCloud-Desktop» |
| `desktop/` | Tauri 2 + React 19 — донор | см. §2–3 |

**Размер донора:** Rust ≈ 69 тыс. строк (из них `chat/` ≈ 18 тыс., `network/` ≈ 20 тыс.), фронт ≈ 122 тыс. строк (961 файл в `components/`).

## 2. Rust (`desktop/src-tauri/src`) — инвентаризация

### 2.1 Сквозная проблема: HTTP-клиент
Весь Rust ходит через `wreq` + `sc-fingerprint` (git-крейт из `SoundCloud-Backend`), то есть с **эмуляцией TLS-отпечатка браузера**. Наши правила это запрещают, поэтому `wreq` везде заменяем на `reqwest` (`socks`, `rustls-tls`, `gzip`, `brotli`, `http2`). `wreq::` встречается в 196 файлах, но больше всего в `chat/` и `network/`, а их мы удаляем целиком. Переписать останется ≈ 10 файлов: `audio/engine.rs`, `track_cache/*`, будущие `api/` и `net/`.

Ещё в `sc_anon` захардкожен User-Agent Linux-Chrome, см. вопрос Q3.

### 2.2 Модули

| Модуль | Решение | Комментарий |
|---|---|---|
| `audio/` (engine, decode, device, output, tick, crossfade, declick, mpeg, timing, state, types) | **оставить** | rodio + cpal + symphonia. `engine.rs` переводим с `wreq` на общий клиент |
| `audio/media_controls.rs` | **оставить** | SMTC через `souvlaki` |
| `audio/{eq*,effects/,pitch,silence,analyser}` | удалить | Эквалайзер, эффекты, pitch (`signalsmith-stretch`), skip-silence и FFT для визуализаторов. Отвязываются чисто, выпиливать дёшево |
| `audio/commands.rs` | переписать | Сейчас фронт рулит очередью и шлёт `audio_load_url`. Станет: очередь в Rust, команды `player_*` |
| `auth/` | переписать | Сейчас это сессия **сервера автора** (`x-session-id`, premium). Новый `auth/`: OAuth-токен SoundCloud из WebView-логина, хранение в Credential Manager |
| `chat/` (131 файл) | удалить | мессенджер автора (`sc-chat`). Из `chat/keychain/wincred.rs` забираем только обёртку над `CredReadW/CredWriteW` (≈ 60 строк), без типов `sc_chat` |
| `discord/` | **оставить** | `discord-rich-presence`. `DISCORD_CLIENT_ID` автора заменить, см. Q5 |
| `import/` | удалить | Spotify / Яндекс / YouTube |
| `local_library/` | удалить | |
| `obs/` | удалить | OBS-оверлей (локальный HTTP-сервер) |
| `scrobble/` | удалить | Last.fm / ListenBrainz, в ТЗ их нет |
| `upload/` | удалить | загрузка треков на сервер автора |
| `diag/` | удалить | отправка диагностики автору; оставляем только локальный лог |
| `network/dns`, `edge`, `health`, `netcheck`, `pro`, `relay_speed`, `call*`, `audio_route`, `fail`, `pace`, `traced*`, `fetch*`, `h2_server`, `server`, `static_server`, `proxy_server`, `wallpapers` | удалить | DoH, relay/edge, star/pro, проверка сети, звонки, обои, локальные HTTP-серверы на `warp` |
| `network/system_proxy.rs` | удалить | Режим «системный прокси» в ТЗ не входит: только Direct и Proxy |
| `network/proxy.rs` (схема `scproxy://`) + `network/image_cache.rs` | переписать | Новая схема `scimg://`: качает через общий клиент, дисковый LRU (`shared/file_lru.rs`), ресайз через суффиксы `-t200x200`/`-t500x500`. Upstream `images.scnative.dev` уходит |
| `track_cache/sc_anon/` | **оставить**, доработать | Скрейп `client_id` (из `__sc_hydration`), circuit breaker, ранжирование транскодингов, HLS. Добавить `Authorization: OAuth`, Go+ превью (`snipped`) вместо отбрасывания, общий клиент |
| `track_cache/state*`, `commands.rs` | **оставить**, упростить | Офлайн-кэш, LRU, pinned/liked, bulk. Убрать ветки «streaming server / premium / refusal ladder» |
| `track_cache/api_download.rs`, `direct_download.rs` | удалить | Скачивание через сервер автора и его storage |
| `track_cache/transcode.rs` | удалить, см. Q4 | ffmpeg-транскодинг в m4a; сам ffmpeg скачивается при первом запуске |
| `app/tray.rs` | **оставить**, переписать меню | Tauri tray (Windows) |
| `app/popover*`, `tray_dot`, `tray_labels` | удалить | Мини-плеер в трее — это **ещё один WebView**, бьёт по цели «≤ 70 МБ в трее». Остаётся нативное меню трея |
| `app/close_action.rs`, `visibility.rs` | переписать | Закрытие в трей = `window.destroy()`, открытие = пересоздание окна |
| `app/autostart/` | оставить только `windows.rs` + `mod.rs` | |
| `app/updater/` | оставить `mod.rs` | Убрать `flatpak.rs`, `mirror.rs` (зеркало через `images.scnative.dev`), упростить `install_kind.rs` под NSIS. В фазе 10 endpoint ведёт на `eidetism/SoundCloud-Desktop` |
| `app/diagnostics.rs`, `log_sink.rs` | **оставить** | Локальный лог + `redact()`, маскирует `OAuth …`, `Bearer …` и подписанные URL. Есть тесты |
| `app/webview2.rs` | оставить | Проверка рантайма WebView2. Ссылку на гайд `docs.getsoundcloud.app` заменить на страницу Microsoft |
| `app/webview_data.rs` | оставить, проверить | Чистит `EBWebView` перед стартом. Профиль логина должен лежать **в другой папке**, иначе пропадёт cookie `datadome` |
| `app/render_mode.rs` | оставить (Windows-часть) | `--disable-gpu`, флаги WebView2 |
| `app/navigation.rs` | упростить | Перехват внешних ссылок; убрать health-пинг `api.scnative.dev` |
| `app/storage/` | упростить | Перенос папки кэша можно оставить, он небольшой |
| `app/hotkeys.rs`, `launch_flags.rs`, `themes*`, `desktop_wallpaper/` | удалить | Глобальные хоткеи заменяют медиаклавиши/SMTC; custom CSS, темы и обои не нужны |
| `shared/{urn,file_lru,blocking}.rs` | оставить | |
| `shared/constants.rs` | переписать | Whitelist доменов сводим к `soundcloud.com`, `api-v2.soundcloud.com`, `*.sndcdn.com`, `*.soundcloud.cloud` (HLS-сегменты, проверить по HAR) |

### 2.3 Cargo-зависимости
**Удаляем:** `sc-fingerprint`, `backend-contracts` (SoundCloud-Backend), `sc-chat` (SoundCloud-Core), `call-client`, `decrypt-client`, `wreq`, `warp`, `hyper-util`, `tower`, `tauri-plugin-http`, `tauri-plugin-global-shortcut`, `ffmpeg-sidecar`, `audiopus`, `ogg`, `biquad`, `signalsmith-stretch`, `rustfft`, `thumbhash`, `if-addrs`, `md-5`, `tokio-rustls`, `webpki-roots`, `windows-registry` (если отпадёт с автостартом), а также всё под `cfg(not(windows))`, macOS и Linux.
**Добавляем:** `reqwest`. Позже, если понадобятся: `specta`/`tauri-specta` (TS-типы) и `windows`/`webview2-com` для событий фреймов WebView2 (детект капчи, см. §5).
**Порог «удалить, если не используется»** проверяем `cargo check` + `cargo machete` (машинно, без установки в проект).

## 3. Фронтенд (`desktop/src`)

Донорский UI построен целиком вокруг API автора: `lib/api*.ts`, `streaming.ts`, `pay-client.ts`, `star-*`, `together`, `chat`, `import*`, `recap`, `lyrics`, `custom-css`, `net/dpi.ts` и т. д. Очередь и плеер живут во фронте (`stores/player.ts`). Новый дизайн не похож на донорский (glass-UI), а архитектура «тонкий фронт, Rust владеет состоянием» требует другого слоя данных.

**Рекомендация: фронт пишем заново** в той же обвязке, донорские компоненты не вычищаем. Из донора берём:
- конфиги: `vite.config.ts`, `tsconfig*.json`, `biome.json`, `index.html`, Tailwind 4;
- зависимости: React 19, zustand, TanStack Query/Virtual, react-router, i18next, lucide-react, sonner, `@fontsource/inter`;
- точечно утилиты: `lib/formatters.ts`, `lib/permalink.ts`, `lib/ids.ts`, `i18n/index.ts` (после проверки).

**Удаляем из `package.json`:** `@dnd-kit/*`, все `@fontsource*` кроме Inter, `@radix-ui/*` (вернём, если понадобятся диалоги/поповеры), `@tauri-apps/plugin-http`, `@tauri-apps/plugin-fs`, `qr-code-styling`, `react-markdown`, `simple-icons`, `react-scan`. Также `tray.html` (popover), `tests/*` (все про чат, импорт и темы), `scripts/*`.

**i18n:** только `ru` и `en`, новые компактные файлы ключей. 2870 строк донорских переводов почти полностью про удаляемые функции.

## 4. Решения (принятые мной, объясняю)

### 4.1 Фаза 1: переносим `desktop/` в корень
После удаления `App/`, `bindings/`, `depens/` и `utils/` в корне остаются только `desktop/`, `docs/`, `LICENSE` и `README`. Единственные ссылки `desktop/` наружу — path-зависимости на `../../utils/*`, а их мы всё равно удаляем. Пути внутри (`frontendDist: ../dist`, `beforeBuildCommand`) относительны `src-tauri/` и при переносе не ломаются. CI переписываем с нуля. Итог: `package.json` и `src-tauri/` в корне, команды без `cd desktop`. Перенос делаем через `git mv`, история сохранится.

### 4.2 `depens/crates/{media-controls,tray,discord-rpc}` не нужны
Это обёртки для RN-шелла. Desktop-донор их **не использует**: у него свои реализации в дереве (`audio/media_controls.rs` на `souvlaki`, `discord/` на `discord-rich-presence`, `app/tray.rs` на Tauri tray). Крейт `tray` вообще на `ksni`, то есть только Linux. Берём реализации из `desktop/`, `depens/` удаляем. Функционально это то же, что просили в ТЗ.

### 4.3 Структура Rust после чистки
```
src-tauri/src/
  net/        клиент reqwest из настроек (Direct/SOCKS5h/HTTP), пересоздание, проверка соединения, локальный SOCKS-форвардер для WebView2
  auth/       окно логина, cookies → oauth_token, /me, keychain (wincred)
  api/        клиент api-v2: client_id, ретраи/backoff/429, модели serde, команды
  player/     очередь, текущий трек, резолв стрима → audio/
  audio/      движок донора (без эффектов)
  track_cache/ офлайн-кэш (sc_anon внутри)
  images/     протокол scimg:// + LRU
  app/        трей, окно (destroy/recreate), автостарт, апдейтер, лог
  discord/
  settings/   настройки (без секретов) в JSON
```

## 5. Логин: технические заметки
- `WebviewWindowBuilder::data_directory(%APPDATA%/dev.eidetism.scdesktop/login-profile)` и `additional_browser_args("--proxy-server=…")`. Для WebView2 другой `data_directory` означает отдельный браузерный процесс, конфликта аргументов с главным окном нет.
- Cookie забираем через `Webview::cookies_for_url("https://soundcloud.com")` (Tauri ≥ 2.4), без JS.
- Капча: верхнеуровневый переход на `captcha-delivery.com` ловим через `on_navigation`. Iframe так не виден, поэтому слушаем `FrameCreated`/`FrameNavigationStarting` у `ICoreWebView2` через `with_webview`. Это нативные события, не инъекция JS. Подсказку показываем отдельным маленьким окном поверх или нативным тултипом.
- SOCKS5 с логином: WebView2 (Chromium) не умеет авторизацию в SOCKS, поэтому поднимаем локальный SOCKS5 без авторизации на `127.0.0.1:<random>`, который пробрасывает через upstream-прокси. HTTP-прокси с логином тоже не авторизуется без диалога, тот же форвардер закрывает и его.

## 6. Нужно от тебя (с указанием фазы)
| Когда | Что |
|---|---|
| до фазы 4 | HAR с soundcloud.com (DevTools → Network → Save all as HAR): **главная** (`/discover` с Mix, Weekly Wave, Daily Drops), **лента** (`/feed`), **лайки**, **история**, **трек с комментариями**, **профиль артиста**, **поиск**. Положить в `docs/har/`. Перед публикацией в репо HAR надо почистить от `oauth_token`/cookies, я это сделаю скриптом и в git сырые HAR не попадут (`docs/har/` в `.gitignore`, в репо только обрезанные фикстуры) |
| до фазы 7 | второй макет — страница трека. В `docs/design/mockup.html` сейчас только главная |
| до фазы 8 | HAR **сообщений**: список диалогов, открытый диалог, отправка сообщения |
| фаза 4 | Discord Application ID, см. Q5 |
| фаза 10 | ключ подписи апдейтов (`pnpm tauri signer generate`) и секреты в репо |

## 7. Открытые вопросы
- **Q1. Фронт с нуля** (рекомендую) или вычищать донорский UI?
- **Q2. Перенос в корень** (рекомендую) — ок?
- **Q3. User-Agent Rust-запросов.** Донор шлёт фиктивный UA Linux-Chrome. Предлагаю брать **реальный UA WebView2** этой машины: читается нативно через `ICoreWebView2Settings::UserAgent` в окне логина, без JS. Тогда токен, полученный в браузере, используется с тем же UA. TLS-отпечаток при этом честный от rustls, не подделываем.
- **Q4. Офлайн-формат без ffmpeg.** Донор качает ffmpeg из интернета при первом запуске и перекодирует всё в m4a. Предлагаю хранить как пришло: progressive MP3 — файлом, HLS — склейкой сегментов (Opus/Ogg и AAC/fMP4), играть через symphonia. Минус: если какой-то HLS-формат symphonia не переварит, такой трек сохраним, только если есть progressive. Плюсы: без бинарника на ≈ 80 МБ, без скачивания исполняемого файла и без лишнего CPU.
- **Q5. Discord RPC.** Нужен свой Application ID на discord.com/developers. Его имя Discord показывает в статусе («Слушает …»). ID автора оставлять нельзя.
- **Q6. Убрать мини-плеер-поповер трея** (рекомендую ради ≤ 70 МБ) — ок? В трее останется меню: play/pause, next/prev, показать окно, выход.

## 8. Риски
- **api-v2 недокументирован** — модели пишем по HAR, не по догадкам. Особенно рискованны личные сообщения (`/users/{id}/conversations`) и `system-playlists`.
- **DataDome на api-v2.** Запросы из Rust с не-браузерным TLS теоретически могут ловить 403. Если поймаем — остановлюсь и обсудим; обходить отпечатки не будем.
- **Память WebView2.** 350 МБ на главной реально только с виртуализацией и маленькими артворками. Замер в фазе 9.
- **`webview_data::wipe_before_launch`** — убедиться, что он не трогает `login-profile`.
