# Сборка MSIX из готового silo-native.exe.
#   .\store\build-msix.ps1 -Exe target-native\release\silo-native.exe -Version 1.1.0
# Для Store: передайте значения из Partner Center (-IdentityName, -Publisher, -PublisherDisplay); Store подпишет пакет сам.
# Для проверки на своём ПК: добавьте -SelfSign (самоподписанный сертификат, ставится в доверенные только если вы сами выполните Import-Certificate).
param(
  [Parameter(Mandatory)] [string]$Exe,
  [Parameter(Mandatory)] [string]$Version,
  [string]$IdentityName = 'Silo.SiloPlayer.LOCALTEST',
  [string]$Publisher = 'CN=SiloLocalTest',
  [string]$PublisherDisplay = 'Silo',
  [string]$Out = 'target-native\msix',
  [switch]$SelfSign
)
$ErrorActionPreference = 'Stop'
$sdk = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Directory | Where-Object { Test-Path "$($_.FullName)\x64\makeappx.exe" } | Sort-Object Name -Descending | Select-Object -First 1
if (-not $sdk) { throw 'Нужен Windows SDK (makeappx.exe)' }
$makeappx = "$($sdk.FullName)\x64\makeappx.exe"; $signtool = "$($sdk.FullName)\x64\signtool.exe"
$ver4 = if ($Version.Split('.').Count -eq 3) { "$Version.0" } else { $Version }
$stage = Join-Path $Out 'stage'
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$stage\assets" | Out-Null
Copy-Item $Exe "$stage\silo-native.exe"
Copy-Item store\assets\*.png "$stage\assets\"
Copy-Item LICENSE, native\THIRD_PARTY_NATIVE.md $stage
(Get-Content store\AppxManifest.xml -Raw -Encoding UTF8).Replace('{{IDENTITY_NAME}}', $IdentityName).Replace('{{PUBLISHER}}', $Publisher).Replace('{{PUBLISHER_DISPLAY}}', $PublisherDisplay).Replace('{{VERSION}}', $ver4) | Set-Content "$stage\AppxManifest.xml" -Encoding UTF8
$pkg = Join-Path $Out "SiloPlayer_${ver4}_x64.msix"
& $makeappx pack /d $stage /p $pkg /o | Out-Null
if ($SelfSign) {
  $cert = New-SelfSignedCertificate -Type Custom -Subject $Publisher -KeyUsage DigitalSignature -FriendlyName 'Silo local test' -CertStoreLocation 'Cert:\CurrentUser\My' -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
  & $signtool sign /fd SHA256 /sha1 $cert.Thumbprint $pkg | Out-Null
  Export-Certificate -Cert $cert -FilePath (Join-Path $Out 'SiloLocalTest.cer') | Out-Null
  Write-Host 'Подписан самоподписанным сертификатом: для установки импортируйте SiloLocalTest.cer в Доверенные люди.'
}
Get-FileHash $pkg -Algorithm SHA256 | Format-List Hash, Path
