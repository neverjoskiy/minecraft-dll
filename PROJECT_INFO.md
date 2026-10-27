# PROJECT: XZ

## Краткое описание

**Название:** XZ  
**Версия:** 0.1.0  
**Язык:** Rust (edition 2024)  
**Тип сборки:** `cdylib` (Windows DLL)  
**Платформа:** Windows (x64)  
**Целевое приложение:** Minecraft (Java-клиент)

Это инжектируемая DLL-внутрянка для Minecraft. После загрузки в процесс игры она подменяет OpenGL-функции для отрисовки собственного оверлея (меню, HUD, ESP), читает состояние игры через JNI и модифицирует байткод классов JVM через JVMTI.

---

## Сборка и зависимости

### Cargo.toml (основное)

```
[package]
name = "XZ"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[profile.release]
panic = "abort"
```

### Зависимости

| Крейт | Назначение |
|-------|------------|
| `egui` | Изменный GUI для оверлея-меню |
| `egui_glow` | Рендер egui через OpenGL (glow) |
| `image` | Загрузка PNG/JPEG текстур и иконок |
| `jni` | Связь с JVM Minecraft |
| `jvmti-bindings` | JVMTI (JVM Tool Interface) для трансформации байткода |
| `min_hook_rs` | Подмена функций (хуки) через MinHook |
| `mlua` | Lua 5.2 (Figura-совместимая среда скриптов) |
| `ristretto_classfile` | Парсинг/трансформация .class файлов JVM |
| `serde`, `serde_json` | Сериализация конфигов |
| `uuid` | Генерация UUID |
| `flate2`, `zip` | Работа с архивами / сжатием |
| `windows` (0.60) | Win32 API: GDI, OpenGL, Foundation, Security, Com, LibraryLoader, Memory, Ole, SystemInformation, SystemServices, Threading, UI (ввод, окна, шелу) |

### Минимальная поддерживаемая платформа

Только **x64 Windows**. `min_hook_rs` требует x64. JVMTI и JNI привязаны к `jvm.dll`目标 процесса.

---

## Архитектура (по модулям)

### Точка входа — `src/lib.rs`

`DllMain` обрабатывает `DLL_PROCESS_ATTACH` и `DLL_PROCESS_DETACH`.

При присоединении к процессу:
1. Выключает уведомления о потоках библиотеки (`DisableThreadLibraryCalls`).
2. Создаёт отдельный поток инициализации (`CreateThread` → `init_thread`).

`init_thread`:
- Создаёт `ModuleRegistry` и регистрирует все модули читов.
- Загружает конфигурацию.
- Инициализирует JVMTI-статус.
- Вызывает `hooks::install()` — подмена OpenGL и оконной процедуры.

При отсоединении (`DLL_PROCESS_DETACH`):
- Сохраняет конфиг.
- Выключает JVMTI (`native_jvmti::shutdown`).
- Выключает хуки (`hooks::shutdown`).

### Реестр модулей — `src/core/module_system.rs`

Трейт `ClientModule`:
```rust
pub trait ClientModule: Send {
    fn menu_module(&self) -> &Module;
    fn menu_module_mut(&mut self) -> &mut Module;
    fn register(&mut self) {}
    fn on_enable(&mut self) {}
    fn on_disable(&mut self) {}
    fn on_frame(&mut self) {}
    fn on_tick(&mut self) {}
    fn on_action_bind(&mut self, _name: &str) {}
    fn load_config(&mut self) {}
    fn save_config(&self) {}
}
```

`ModuleRegistry` управляет списком модулей, их тиком, кадром, биндами клавиш, сохранением/загрузкой конфига, HUD-снапшотом.

### Список модулей — `src/modules/mod.rs`

| Модуль | Назначение (по названию в коде) |
|--------|---------------------------------|
| `AttackAuraModule` | Автоатака вокруг игрока |
| `AimAssistModule` | Ассист прицеливания (aimbot) |
| `AutoSprintModule` | Автобег |
| `ChamsModule` | Подсветка сквозь стены (chams) |
| `ChangeTimeModule` | Изменение времени мира |
| `ChinaHatModule` | Над головой игрока (китайская шляпа) |
| `FullBrightModule` | Яркость Always (fullbright) |
| `HudModule` | Экранный HUD |
| `JumpCircleModule` | Кружок прыжка |
| `KeyPearlModule` | Эндер-жемчуг по бинду |
| `NameTagsModule` | Имена игроков поверх мира |
| `NoFriendDamageModule` | Отключение урона по друзьям |
| `NoJumpDelayModule` | Без задержки прыжка |
| `NoOverlayModule` | Отключение оверлея |
| `SkyShaderModule` | Шейдер неба |
| `TargetEspModule` | ESP цели |
| `TrajectoriesModule` | Траектории снарядов |
| `TriggerBotModule` | Автострельба при наведении |

### GUI / оверлей — `src/gui/`

Рендерится через `egui` + `glow` внутри хука `wglSwapBuffers`.

- `menu.rs` — основное меню с разделами (Combat, Render и др.), карточками модулей, биндами, конфигами, темой.
- `hud.rs` — отрисовка HUD (watermark, potions, target, armor, staff, active keybinds).
- `renderer.rs` — `OverlayRenderer`, отрисовка примитивов egui, иконок предметов.
- `input.rs` — синхронизация ввода мыши, обновление состояния.
- `draw.rs` — вспомогательная отрисовка: aim FOV, name tags, dim background, world markers, action wheel, aim overlay, neuro progress.
- `sky_shader.rs` — шейдер неба, перехват отрисовки неба.
- `blur.rs` — размытие регионов (меню, HUD).
- `theme.rs` — темы оформления, палитры.
- `anim.rs` — анимации меню и переключателей.
- `icons.rs` — иконки категорий модулей.
- `widgets.rs` — кастомные виджеты (слайдеры, чекбоксы, цветовые пикеры).
- `avatar_preview.rs` — предпросмотр аватара (Figura-стиль).
- `file_dialog.rs` — диалог файлов.
- `system_stats.rs` — статистика системы (TPS, FPS и т.п.).

### Подмена OpenGL — `src/platform/hooks.rs`

Используется `min_hook_rs`.

1. **`wglSwapBuffers`** — хук на смену буферов OpenGL. Внутри вызывается `gui::render(hdc)` — отрисовка всего меню/HUD. Затем вызывается оригинальная функция.
2. **Графические хуки** — перехваты `glDrawArraysInstanced`, `glDrawArraysInstancedBaseInstance`, `glDrawElementsInstancedBaseVertex`, `glDrawElementsInstancedBaseVertexBaseInstance`. Используются для перехвата отрисовки мира (например, подмена неба через `gui_sky_shader`).
3. **Оконная процедура** — `SetWindowLongPtrW(hwnd, GWLP_WNDPROC, overlay_wnd_proc)`. Перехватывает все сообщения окна Minecraft:
   - Клавиатура (KEYDOWN/KEYUP, SYSKEYDOWN/SYSKEYUP, CHAR).
   - Мышь (кнопки, колёсико, движение).
   - Меню переключается по `RShift`.
   - При открытом меню ввод игры блокируется (`should_block_game_input` возвращает `LRESULT(0)`).

### JNI-мост — `src/platform/jni_bridge.rs`

- Открывает доступ к запущенной JVM через `JNI_GetCreatedJavaVMs` из `jvm.dll`.
- Находит экземпляр Minecraft: `net.minecraft.client.Minecraft.getInstance()`.
- Читает поля игры через JNI: `level`, `player`, `gui`, `screen`, `overlay`, `mouseHandler`.
- Функции:
  - `is_game_active()` — есть ли мир и нет ли экрана.
  - `is_world_active()` — есть ли уровень и игрок.
  - `release_mouse_for_overlay()` — освобождает мышь для меню (снимает захват, сбрасывает курсор).
  - `grab_mouse_after_overlay()` — возвращает захват мыши после закрытия меню.
  - `keep_afk_alive()` — имитирует ввод, чтобы не отключать по AFK.
  - `minecraft_instance()` — получение объекта Minecraft.
  - `load_class()` / `load_game_class()` — загрузка классов игры через class loader.

### JVMTI — `src/platform/jvmti/`

Использует `jvmti-bindings` для работы с JVM Tool Interface.

- **`mod.rs`** — агент JVMTI (`NativeAgent` с `class_file_load_hook_with_jvmti`). При загрузке классов игры проверяет, есть ли хук для данного класса (`transformer::has_hook_for`), дампит оригинальный байткод, валидирует профиль цели, трансформирует байткод через `ristretto_classfile`, аллоцирует новый байткод в JVM и подменяет класс через JVMTI `RetransformClasses`.
- **`transformer/mod.rs`** — регистрация хуков на классы, трансформация `.class` файлов, список за hook-ленных классов.
- **`profiles.rs`** — профили версий Minecraft (GAME_VERSION, методы рендеринга, валидация целевого класса).
- **`status.rs`** — статус JVMTI (установлен ли, включены ли события, количество трансформированных классов, ошибки).
- **`trampoline.rs`** — трамплины для вызова оригинальных методов после хука.

### Антидетект — `src/anti_detect/`

- **`injgen.rs`** — сканирование образа `jvm.dll` в памяти, поиск определённых сигнатур (`signature_a`, `signature_b`) в RW-регионах и их затирание (`scrub_dword` записью 0). Функции `neutralize` / `neutralize_periodic` вызываются в цикле отрисовки.
- **`jvm_image.rs`** — получение базового адреса и размера образа `jvm.dll` через парсинг PE-заголовка (`IMAGE_DOS_HEADER`, `IMAGE_NT_HEADERS64`, `SizeOfImage`).

### Ядро — `src/core/`

- **`config.rs`** — система конфигураций. Сохраняет/загружает JSON в временную папку (локатор `.xz_cfg_root`), папка называется случайно (`xz_<hex>`). Поддерживает несколько конфигов, добавление/удаление, активный конфиг, друзей, endpoint Figura. Атомная запись через `.tmp` + `rename`.
- **`diag.rs`** — диагностика и логирование.
- **`interface.rs`** — определения `Module`, `Setting` (Checkbox, Slider, ColorPicker, ActionBind, Choice), `Bind` (Toggle/Hold, целевое состояние), `Category`.
- **`theme_colors.rs`** — цвета темы,公開руемые для других модулей.

### Утилиты — `src/utils/`

- `aim_target.rs` — выбор цели для aim/trigger.
- `aura/` — логика ора attack aura.
- `block_esp.rs` — ESP блоков.
- `friends.rs` — список друзей.
- `geometry.rs` — векторная математика (Vec3d и т.п.).
- `hud_data.rs` — данные для HUD.
- `jump_circle.rs` — визуализация круга прыжка.
- `minecraft.rs` — контекст Minecraft, время игры, проверка экранов.
- `name_tags.rs` — сбор и отрисовка имён над головами, скрытие имён ванили.
- `neuro.rs` — нейро-оверлей (прогресс).
- `player_chams.rs` / `player_esp.rs` / `player_esp_render.rs` — подсветка и ESP игроков.
- `player_face.rs` — лица игроков (иконки).
- `potions.rs` — активные эффекты зелий.
- `random.rs` — генератор случайных чисел.
- `rotation.rs` — работа с поворотами (yaw/pitch).
- `settings.rs` — общие настройки.
- `sprint_control.rs` — управление бегом.
- `target_esp_fx.rs` — эффекты ESP цели.
- `time.rs` / `tps.rs` — время и тики в секунду.
- `trajectories.rs` — траектории снарядов.
- `world_markers.rs` — маркеры мира (TargetESP, ChinaHat и т.п.).

### Аватар / Figura — `src/avatar/`

Поддержка Figura-аватаров:
- `action_wheel.rs` — колесо действий (открывается по клавише `Insert` / `0x2D`, закрывается `Esc` / `0x1B`).
- `animation/` — анимации.
- `format/` — формат аватара.
- `lua/` — Lua 5.2 среда (через `mlua`, Figura-совместимая).
- `math/` — математическая библиотека.
- `model/` — модель аватара.
- `network/` — сетевая часть Figura.
- `permissions/` — права доступа.
- `render/` — рендер аватара.
- `runtime/` — runtime аватара.

### Мир и рендер — `src/world_render/`

- `hooks.rs` — хуки рендеринга мира.
- `geometry.rs` — геометрия мира.
- `mod.rs` — `ensure_armed()` и инициализация мирового рендера.

---

## Цикл работы при каждом кадре

1. Minecraft вызывает `wglSwapBuffers`.
2. Хук `hooked_wgl_swap_buffers` перехватывает вызов.
3. Вызывается `gui::render(hdc)`:
   - Обновляется ввод (клавиатура, мышь из `overlay_wnd_proc`).
   - Проверяется активность мира через JNI (`is_world_active`, `is_game_active`).
   - Обрабатываются бинды клавиш модулей.
   - Тикают/modules (`on_tick`, `on_frame`).
   - Собираются данные HUD, имя тегов, ESP, aim overlay, action wheel.
   - Рендерится меню egui (если открыто).
   - Рендерятся иконки предметов, лиц, зелий.
4. Вызывается оригинальный `wglSwapBuffers` — кадр отображается в игре вместе с оверлеем.

---

## Файлы ресурсов (`resources/`)

| Файл | Назначение |
|------|------------|
| `circle1.svg` | SVG-графика (вероятно, для прицела/оверлея) |
| `figma/` | Дизайн-макеты интерфейса |
| `icons/` | Иконки |
| `Keybinds.ttf` | Шрифт иконок биндов |
| `menu_icons.ttf` | Шрифт иконок меню |
| `Potions.ttf` | Шрифт иконок зелий |
| `profile_avatar.png` | Аватар профиля в меню |
| `profile_banner.jpg` | Баннер профиля |
| `profile_banner.mp4` | Видео-баннер профиля |
| `target_esp.png` | Текстура маркера цели ESP |

---

## Конфигурация

Конфиги хранятся как JSON во временной папке пользователя. Локатор `.xz_cfg_root` содержит имя папки. Формат конфига:
- `version` — версия формата (1)
- `theme` — активная тема
- `friends` — список друзей
- `modules` — для каждого модуля: имя, enabled, bind, настройки
- `figura_endpoint` — endpoint для Figura

Поддерживаются типы настроек: Checkbox, Slider, ColorPicker, ActionBind, Choice.

---

## Тестовые фикстуры

В `tests/fixtures/` находятся фикстуры для тестов (вероятно, примеры `.class` файлов для JVMTI-трансформера).

---

## Инжектор (`injector/`)

Отдельный бинарный crate (edition 2024, Windows x64) для инжекции `XZ.dll` в процесс Minecraft.

- Методы: `remote-thread` (CreateRemoteThread + LoadLibraryA) и `manual` (ручной маппинг PE).
- CLI: `--process`, `--pid`, `--dll`, `--method`, `--wait`, `--launch` (JVMTI `-agentpath`), `--gui`.
- Без аргументов запускается GUI-окно (поля процесс/PID/DLL/метод/wait и кнопка Inject).
- Проверка архитектуры x64, повышение прав (SeDebugPrivilege), логи/конфиг в `%TEMP%\injector\`.
- Сборка/тесты: `.github/workflows/build-injector.yml` (windows-latest, cargo test, cargo build --release, артефакт injector.exe).

## TriggerBot (обновление)

- Перенесены улучшения из neamaz: cooldown с учётом onGround (пороги 0.92/0.95 и интервалы 120/400 мс), проверка блокирования игрока (no-hit block), сброс спринта перед атакой при включённом `Sprint control`.
- Сброс спринта выполняется через `minecraft::set_player_sprinting(ctx, false)` в `perform_attack`.
- Не перенесены: shift-tap и attack-key hold/release (требуют отдельные модули).

## Ответственность

Проект предназначен для модификации клиента Minecraft, что может нарушать условия использования игры и привести к блокировке аккаунта. Использование на свой страх и риск.
