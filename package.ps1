# Script di packaging e rilascio di IManCraft
$ErrorActionPreference = "Stop"

Write-Host "══════════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host " CREAZIONE PACCHETTO DI DISTRIBUZIONE IMANCRAFT v0.2.0" -ForegroundColor Cyan
Write-Host "══════════════════════════════════════════════════════════════" -ForegroundColor Cyan

$distDir = "dist\IManCraft-v0.2.0-win64"
$zipPath = "dist\IManCraft-v0.2.0-win64.zip"

Write-Host "`n[1/4] Compilazione eseguibili in Release mode..." -ForegroundColor Yellow
cargo build --release -p imancraft -p imancraft-cli

if (-not (Test-Path "target\x86_64-pc-windows-gnu\release\imancraft.exe")) {
    Write-Error "imancraft.exe non trovato in target\x86_64-pc-windows-gnu\release\"
}
if (-not (Test-Path "target\x86_64-pc-windows-gnu\release\imancraft-cli.exe")) {
    Write-Error "imancraft-cli.exe non trovato in target\x86_64-pc-windows-gnu\release\"
}

Write-Host "[2/4] Preparazione cartella di distribuzione: $distDir..." -ForegroundColor Yellow
if (Test-Path $distDir) {
    Remove-Item -Recurse -Force $distDir
}
New-Item -ItemType Directory -Path $distDir -Force | Out-Null

Copy-Item "target\x86_64-pc-windows-gnu\release\imancraft.exe" $distDir\
Copy-Item "target\x86_64-pc-windows-gnu\release\imancraft-cli.exe" $distDir\
Copy-Item "lib\sqlite3.dll" $distDir\
Copy-Item "assets\imancraft.ico" $distDir\
Copy-Item "LEGGIMI.txt" $distDir\
if (Test-Path "tools") {
    Copy-Item "tools" -Destination $distDir -Recurse
}

# Launcher rapido
$batContent = "@start `"`" `"%~dp0imancraft.exe`""
Set-Content -Path "$distDir\Avvia IManCraft.bat" -Value $batContent -Encoding Ascii

Write-Host "[3/4] Creazione archivio ZIP: $zipPath..." -ForegroundColor Yellow
if (Test-Path $zipPath) {
    Remove-Item -Force $zipPath
}
Compress-Archive -Path "$distDir\*" -DestinationPath $zipPath -CompressionLevel Optimal

Write-Host "[4/4] Verifica e calcolo checksum SHA-256..." -ForegroundColor Yellow
Write-Host "`nContenuto del pacchetto:" -ForegroundColor Green
Get-ChildItem $distDir | ForEach-Object {
    $hash = (Get-FileHash $_.FullName -Algorithm SHA256).Hash
    $sizeKb = [math]::Round($_.Length / 1KB, 1)
    Write-Host ("  {0,-22} {1,10} KB  SHA256: {2}" -f $_.Name, $sizeKb, $hash)
}

$zipHash = (Get-FileHash $zipPath -Algorithm SHA256).Hash
$zipSizeMb = [math]::Round((Get-Item $zipPath).Length / 1MB, 2)
Write-Host "`nArchivio finale creato con successo:" -ForegroundColor Green
Write-Host ("  {0,-22} {1,10} MB  SHA256: {2}" -f (Get-Item $zipPath).Name, $zipSizeMb, $zipHash)
Write-Host "`n══════════════════════════════════════════════════════════════`n" -ForegroundColor Cyan
