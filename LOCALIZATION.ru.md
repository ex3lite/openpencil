# OpenPencil — русская локализация для Codex

Форк [ZSeven-W/openpencil](https://github.com/ZSeven-W/openpencil), ветка `codex/russian-localization`, стабильная база `d155cd08c09cdb79f4c0a2c733dc5f9376765abd` (0.8.5).

Исправлен протокол встраивания: `?locale=ru` и сообщение `op-bridge/locale` принимают русский и все остальные канонические языки редактора. Раньше принимались только китайский и английский. Переведены и отредактированы 129 оставшихся строк русского каталога, включая стартовую страницу, шаблоны, подключение моделей и действия Git. Названия продуктов, протоколов и примеры адресов сохранены. Каталог содержит 1852 ключа; тест запрещает появление непереведённых английских и китайских строк, кроме явного списка технических обозначений.

## Проверка и сборка

Нужны Rust из `rust-toolchain.toml`, Node.js, Python 3, wasm-bindgen 0.2.117 и Binaryen (`wasm-opt`). Все инструменты бесплатны.

```sh
git submodule update --init --recursive
cargo test -p op-editor-core bridge_protocol --locked
cargo test -p op-i18n --locked
cargo test -p op-host-web --no-default-features --features canvaskit host_locale_query --locked
python3 tools/export-i18n-catalogs.py --check
bash tools/check-wasm-bundle.sh
```

Результат — `crates/op-host-web/pkg/`, включая `assets/i18n/ru.json`. Проверка сборки контролирует отсутствие импортов `env.*`, корректность каталогов и лимит размера WASM. Для подключения к Codex используйте [русский форк плагина](https://github.com/ex3lite/codex-openpencil/tree/codex/russian-localization) и его `scripts/localize-runtime.mjs`.

Макеты и пользовательские настройки не включены в исходники. Лицензия исходного проекта — MIT; авторство ZSeven-W и лицензии зависимостей сохранены.
