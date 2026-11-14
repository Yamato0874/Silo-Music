# Скачивает официальный yt-dlp (yt-dlp_win.zip, папка onedir) с github.com/yt-dlp/yt-dlp/releases и сверяет SHA-256 с официальным списком SHA2-256SUMS.
# Результат: native\installer\bin\yt-dlp\yt-dlp.exe (в git не попадает); его кладёт рядом с программой установщик. Запуск onedir-версии в 3 раза быстрее одного exe.
$ErrorActionPreference = 'Stop'
$dir = Join-Path $PSScriptRoot '..\native\installer\bin'
New-Item -ItemType Directory -Force $dir | Out-Null
$base = 'https://github.com/yt-dlp/yt-dlp/releases/latest/download'
$zip = Join-Path $dir 'yt-dlp_win.zip'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$wc = New-Object Net.WebClient
$wc.DownloadFile("$base/yt-dlp_win.zip", $zip)
$sums = $wc.DownloadString("$base/SHA2-256SUMS")
$want = ($sums -split "`n" | Where-Object { $_ -match ' yt-dlp_win\.zip\s*$' } | Select-Object -First 1).Split(' ')[0].Trim().ToLower()
$got = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
if ($want -ne $got) { Remove-Item $zip; throw "Контрольная сумма не совпала: ожидали $want, получили $got" }
$to = Join-Path $dir 'yt-dlp'
if (Test-Path $to) { Remove-Item -Recurse -Force $to }
Expand-Archive $zip -DestinationPath $to
Remove-Item $zip
Write-Host "yt-dlp проверен ($got): $to"
