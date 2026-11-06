# SC Desktop

Fork of [zxcloli666/SoundCloud-Desktop](https://github.com/zxcloli666/SoundCloud-Desktop).

Личный десктоп-клиент SoundCloud для Windows 11. Ходит напрямую в SoundCloud (без промежуточных серверов), напрямую или через пользовательский SOCKS5/HTTP-прокси.

Стек: Tauri 2 (Rust) + React 19 + TypeScript + Vite + Tailwind 4.

## Разработка

```bash
pnpm install
pnpm tauri dev
```

Проверки:

```bash
pnpm tsc --noEmit
pnpm biome check src/
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

План работ — [docs/FORK_PLAN.md](docs/FORK_PLAN.md).

## Лицензия

MIT. Исходный код © zxcloli666, см. [LICENSE](LICENSE).
