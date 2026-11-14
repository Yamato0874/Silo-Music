# Скачивает официальный yt-dlp.exe с github.com/yt-dlp/yt-dlp/releases и сверяет контрольную сумму с официальным списком SHA2-256SUMS.
# Результат: native\installer\bin\yt-dlp.exe (в git не попадает). Его подхватывает установщик, а для сборки exe "всё в одном" путь передаётся в SILO_YTDLP_PATH.
$ErrorActionPreference = 'Stop'
$dir = Join-Path $PSScriptRoot '..\native\installer\bin'
New-Item -ItemType Directory -Force $dir | Out-Null
$base = 'https://github.com/yt-dlp/yt-dlp/releases/latest/download'
$exe = Join-Path $dir 'yt-dlp.exe'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$wc = New-Object Net.WebClient
$wc.DownloadFile("$base/yt-dlp.exe", $exe)
$sums = $wc.DownloadString("$base/SHA2-256SUMS")
$want = ($sums -split "`n" | Where-Object { $_ -match ' yt-dlp\.exe\s*$' } | Select-Object -First 1).Split(' ')[0].Trim().ToLower()
$got = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLower()
if ($want -ne $got) { Remove-Item $exe; throw "Контрольная сумма не совпала: ожидали $want, получили $got" }
Write-Host "yt-dlp.exe проверен ($got): $exe"
$p = (Resolve-Path $exe).Path.Replace([string][char]92, "/")
Write-Host "Exe всё в одном: `$env:SILO_YTDLP_PATH = '$p'; cargo build --release -p silo-native --features embed-ytdlp"
