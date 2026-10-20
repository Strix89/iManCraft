# Script di packaging e rilascio di IManCraft
$ErrorActionPreference = "Stop"

Write-Host "══════════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host " CREAZIONE PACCHETTO DI DISTRIBUZIONE IMANCRAFT v0.2.0" -ForegroundColor Cyan
Write-Host "══════════════════════════════════════════════════════════════" -ForegroundColor Cyan

$distDir = "dist\IManCraft-v0.2.0-win64"
$zipPath = "dist\IManCraft-v0.2.0-win64.zip"

Write-Host "`n[1/4] Compilazione eseguibili in Release mode..." -ForegroundColor Yellow
cargo build --release -p imancraft -p imancraft-cli

$releaseDir = "target\release"
if (-not (Test-Path "$releaseDir\imancraft.exe")) {
    $releaseDir = "target\x86_64-pc-windows-gnu\release"
}

if (-not (Test-Path "$releaseDir\imancraft.exe")) {
    Write-Error "imancraft.exe non trovato in $releaseDir\"
}
if (-not (Test-Path "$releaseDir\imancraft-cli.exe")) {
    Write-Error "imancraft-cli.exe non trovato in $releaseDir\"
}

$stageDir = "dist\package_staging"
Write-Host "[2/4] Preparazione cartella di staging: $stageDir..." -ForegroundColor Yellow
if (Test-Path $stageDir) {
    Remove-Item -Recurse -Force $stageDir
}
New-Item -ItemType Directory -Path $stageDir -Force | Out-Null

Copy-Item "$releaseDir\imancraft.exe" $stageDir\
Copy-Item "$releaseDir\imancraft-cli.exe" $stageDir\
Copy-Item "lib\sqlite3.dll" $stageDir\
Copy-Item "assets\imancraft.ico" $stageDir\
Copy-Item "LEGGIMI.txt" $stageDir\
if (Test-Path "tools") {
    Copy-Item "tools" -Destination $stageDir -Recurse -Force
}

# Launcher rapido
$batContent = "@start `"`" `"%~dp0imancraft.exe`""
Set-Content -Path "$stageDir\Avvia IManCraft.bat" -Value $batContent -Encoding Ascii

Write-Host "[3/4] Creazione archivio ZIP: $zipPath..." -ForegroundColor Yellow
if (Test-Path $zipPath) {
    Remove-Item -Force $zipPath
}
Compress-Archive -Path "$stageDir\*" -DestinationPath $zipPath -CompressionLevel Optimal

# Tentativo di sincronizzare anche la cartella distDir scompattata (se non bloccata da un'istanza aperta)
try {
    if (-not (Test-Path $distDir)) {
        Copy-Item -Path $stageDir -Destination $distDir -Recurse -Force
    }
} catch {
    # File in uso da parte dell'utente, archivio ZIP comunque aggiornato
}

Write-Host "[4/4] Verifica e calcolo checksum SHA-256..." -ForegroundColor Yellow
Write-Host "`nContenuto del pacchetto:" -ForegroundColor Green
Get-ChildItem $stageDir | ForEach-Object {
    $hash = (Get-FileHash $_.FullName -Algorithm SHA256).Hash
    $sizeKb = [math]::Round($_.Length / 1KB, 1)
    Write-Host ("  {0,-22} {1,10} KB  SHA256: {2}" -f $_.Name, $sizeKb, $hash)
}

$zipHash = (Get-FileHash $zipPath -Algorithm SHA256).Hash
$zipSizeMb = [math]::Round((Get-Item $zipPath).Length / 1MB, 2)
Write-Host "`nArchivio finale creato con successo:" -ForegroundColor Green
Write-Host ("  {0,-22} {1,10} MB  SHA256: {2}" -f (Get-Item $zipPath).Name, $zipSizeMb, $zipHash)
Write-Host "`n══════════════════════════════════════════════════════════════`n" -ForegroundColor Cyan
