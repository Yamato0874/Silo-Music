# Silo Player: выпуск в Microsoft Store и на GitHub

## 1. Что уже готово локально
- `store/AppxManifest.xml`: шаблон манифеста MSIX (полное доверие, доступ в интернет, ru-RU и en-US).
- `store/assets/`: значки 44, 71, 150, 310, широкая плитка 310x150, логотип магазина, заставка (сделаны из `app-icon.png`).
- `store/build-msix.ps1`: собирает `SiloPlayer_<версия>_x64.msix` из готового `silo-native.exe` (проверено: пакет собирается, около 6 МБ). С ключом `-SelfSign` подписывает тестовым сертификатом для проверки на своём ПК.
- Установщик NSIS `Silo Player Native_<версия>_x64-setup.exe` и контрольные суммы уже выкладываются в GitHub Releases.

## 2. Что нужно от владельца (я не регистрируюсь и не плачу за него)
1. Аккаунт разработчика в [Partner Center](https://partner.microsoft.com/dashboard) (для физлица разовый взнос, сумму смотрите при регистрации).
2. Зарезервировать имя «Silo Player» в Partner Center. Там же взять **Package/Identity/Name**, **Package/Identity/Publisher** и **Publisher display name**: их подставляют в `build-msix.ps1` (`-IdentityName`, `-Publisher`, `-PublisherDisplay`).
3. Ответить на возрастную анкету IARC (музыкальный плеер без насилия и покупок, обычно 3+, но ответы даёт владелец).
4. Указать страну продаж и цену (бесплатно).

## 3. Требования Store и что к ним готово
| Требование | Состояние |
|---|---|
| Политика конфиденциальности по ссылке | есть: https://yamato0874.github.io/Silo-Music/privacy.html |
| Условия использования | есть: https://yamato0874.github.io/Silo-Music/terms.html |
| Описание, ключевые слова, скриншоты (минимум 1, лучше 4, 1920x1080) | текст ниже, скриншоты из режима `SILO_DEMO` делаются по запросу |
| Значки | готовы, перед отправкой стоит заменить на оформленные дизайнером |
| Подпись пакета | Store подписывает MSIX сам, свой сертификат не нужен |
| Проверка Windows App Certification Kit (WACK) | запускается на готовом MSIX, ещё не гонялась |

## 4. Главный риск: режим yt-dlp
Режим «Библиотека yt-dlp» получает звук и видео YouTube не через официальный плеер, это нарушает условия YouTube. Приложение с такой функцией скорее всего не пройдёт модерацию Store (политики требуют соблюдать условия сторонних сервисов и ограничивают загрузку исполняемого кода). Кнопка установки Apollo скачивает Python и PyTorch, это тоже поведение, которое Store может отклонить.

**Предложение:** две сборки из одного кода.
- **GitHub (полная)**: как сейчас, yt-dlp и Apollo.
- **Store (чистая)**: Cargo-функция `store`, при которой режим yt-dlp и установка Apollo отключены, остаётся «Чистый YouTube» (официальный плеер), SoundCloud, Jamendo и локальные файлы. Эта сборка реализуется отдельной задачей в `dev` (около одного дня работы), без неё MSIX отправлять не стоит.

## 5. Размещение на GitHub
- Релиз `vX.Y.Z` содержит: установщик `.exe`, портативный `silo-native.exe` (один файл, ставить не нужно), `SHA256SUMS.txt`. Позже добавить `.msix` для сторонней установки (с подписью, иначе Windows не поставит).
- Подпись кода exe: сертификат OV/EV (платный) или бесплатная подпись [SignPath Foundation](https://signpath.org/) для открытых проектов. Без подписи Windows SmartScreen показывает предупреждение при первом запуске. Для Store отдельная подпись не нужна.
- Автообновление: ссылка «Скачать последнюю» ведёт на `/releases/latest`.

## 6. Текст листинга
**Название:** Silo Player

**Краткое описание (RU):** Локальный музыкальный плеер: «Эхо», караоке, апскейл звука, без аккаунтов и слежки.

**Описание (RU):** Silo Player это быстрый нативный плеер на Rust без веб-движка. «Эхо» собирает бесконечный поток под ваш вкус и учится на том, что вы слушаете и пропускаете. Встроенный движок звука: эквалайзер, кроссфейд, gapless, скорость без смены тона, выравнивание громкости. Караоке с бегущим текстом, импорт плейлистов (M3U, PLS, CSV, JSON), 7 тем, русский и английский интерфейс. Нет аккаунтов и телеметрии, все данные лежат на вашем компьютере. Приложение с открытым кодом.

**Short description (EN):** A local music player with Echo radio, karaoke and audio upscaling. No accounts, no tracking.

**Description (EN):** Silo Player is a fast native Rust music player with no web engine inside. Echo builds an endless stream around your taste. Own audio engine: equalizer, crossfade, gapless playback, speed without pitch change, volume normalization. Karaoke with scrolling lyrics, playlist import (M3U, PLS, CSV, JSON), 7 themes, Russian and English interface. No accounts or telemetry; everything stays on your computer. Open source.

**Ключевые слова:** музыка, плеер, караоке, эквалайзер, эхо, плейлисты, music, player, karaoke, equalizer

**Что нового (1.1.0):** см. `CHANGELOG.md`.

## 7. Порядок действий
1. Владелец заводит аккаунт и резервирует имя, присылает три значения идентичности.
2. Реализую сборку `store` (без yt-dlp и Apollo) в ветке от `dev`.
3. Собираю MSIX с настоящей идентичностью, прогоняю WACK.
4. Подготавливаю скриншоты и загружаю пакет в Partner Center вручную владельцем (отправка на сертификацию остаётся за ним).
