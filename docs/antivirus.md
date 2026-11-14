# Ложные срабатывания антивирусов

Silo Player распространяется как неподписанный exe на Rust. Такие файлы часто получают эвристические срабатывания (`Evo-gen`, `Suspicious.low.ml.score` и подобные), хотя вредоносного кода в них нет: исходный код открыт, сборка воспроизводима (`README.md`), контрольные суммы лежат в каждом релизе.

## Отчёт VirusTotal по 1.3.0 (портативный exe, sha256 `89d1963e…be9501`)
5 из 69 движков: Avast и AVG (`Win64:Evo-gen [Trj]`), Avira (`TR/W64.Evo`), WithSecure (`Trojan.TR/W64.Evo`), Trapmine (`Suspicious.low.ml.score`). Все пять это общие эвристические или ML-сигнатуры, а не конкретное семейство вредоноса. Вероятная причина: внутри exe лежал упакованный исполняемый файл (yt-dlp, 23 МБ), который программа распаковывала на диск. В 1.3.1 так больше не делается.

## Что сделано в 1.3.1
- yt-dlp не вшит в exe: установщик кладёт официальный `yt-dlp.exe` отдельным файлом, портативный exe скачивает его при первом запуске с проверкой SHA-256 по официальному списку.
- В exe добавлены манифест (уровень `asInvoker`, Windows 10 и 11) и сведения о версии.

## Что остаётся (нужно действие владельца)
- **Подпись кода.** Бесплатно для открытых проектов: [SignPath Foundation](https://signpath.org/) (нужна заявка владельца репозитория). Платно: сертификат OV или EV. Подпись сильнее всего снижает срабатывания и предупреждение SmartScreen.
- **Отправка ложных срабатываний.** Тексты ниже. Отправляет владелец (форма просит файл или хеш и контактную почту), я их не отправляю.

## Текст обращения (RU)
> Прошу проверить файл как ложное срабатывание. Silo Player 1.3.1, открытый музыкальный плеер для Windows на Rust (https://github.com/Yamato0874/Silo-Music, лицензия MIT). SHA-256: <хеш файла>. Ваш движок определяет файл как «<имя сигнатуры>», это общая эвристика. Файл не содержит вредоносного кода: исходники открыты, сборка воспроизводима из репозитория, программа не требует прав администратора, не закрепляется в системе, не собирает пароли и не отправляет личные данные. Единственная загрузка из сети: поиск и воспроизведение музыки и скачивание официального yt-dlp с github.com/yt-dlp/yt-dlp/releases с проверкой SHA-256. Прошу исключить файл из детектирования и скорректировать сигнатуру.

## Text for the vendor (EN)
> Please review this file as a false positive. Silo Player 1.3.1 is an open-source Windows music player written in Rust (https://github.com/Yamato0874/Silo-Music, MIT license). SHA-256: <file hash>. Your engine flags it as "<signature name>", a generic heuristic. The file contains no malicious code: the source is public and the build is reproducible from the repository, the app runs without administrator rights, does not persist in the system, does not collect passwords and does not send personal data. Its only downloads are music search/playback and the official yt-dlp release from github.com/yt-dlp/yt-dlp/releases, verified by SHA-256. Please whitelist the file and adjust the signature.

## Куда отправлять (проверьте актуальные адреса на сайтах производителей)
- Microsoft Defender: https://www.microsoft.com/wdsi/filesubmission
- Avast и AVG: форма «False positive» на сайте avast.com
- Avira: https://www.avira.com/en/analysis/submit
- WithSecure: форма отправки образцов на withsecure.com
- Trapmine: через поддержку на сайте производителя
- Общий путь: на странице файла в VirusTotal кнопка «Comments» и ссылки производителей
