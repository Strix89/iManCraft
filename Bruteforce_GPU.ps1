$Host.UI.RawUI.WindowTitle = "iManCraft - Bruteforce Note Apple (GPU NVIDIA RTX 3060 Ti)"
Write-Host "===============================================================================" -ForegroundColor Green
Write-Host "  iManCraft - RECUPERO PASSWORD NOTE APPLE TRAMITE GPU (RTX 3060 Ti)" -ForegroundColor Green
Write-Host "===============================================================================" -ForegroundColor Green
Write-Host ""
Write-Host " Target:        NoteStore.sqlite (Backup 00008110-000A648136D0A01E)" -ForegroundColor Cyan
Write-Host " Note Protette: 6 note cifrate" -ForegroundColor Cyan
Write-Host " Indizio:       `"nulla e' reale, tutto e' lecito`"" -ForegroundColor Yellow
Write-Host " Acceleratore:  NVIDIA GeForce RTX 3060 Ti (OpenCL 3.0 CUDA)" -ForegroundColor White
Write-Host " Velocita':     ~80.000 - 90.000 combinazioni al secondo" -ForegroundColor White
Write-Host " Spazio PIN:    8 cifre (100.000.000 combinazioni, da 00000000 a 99999999)" -ForegroundColor White
Write-Host " Tempo stimato: ~18 - 20 MINUTI (invece di oltre 6 ore su CPU!)" -ForegroundColor Green
Write-Host "===============================================================================" -ForegroundColor Green
Write-Host ""
Write-Host "Comandi interattivi durante il calcolo:" -ForegroundColor Gray
Write-Host "  [s] = Aggiorna stato e percentuale attuale" -ForegroundColor Gray
Write-Host "  [p] = Pausa  |  [r] = Riprendi  |  [q] = Esci" -ForegroundColor Gray
Write-Host ""
Write-Host "Avvio calcolo su GPU in corso..." -ForegroundColor Yellow
Write-Host ""

$hashcatDir = Join-Path $PSScriptRoot "tools\hashcat-6.2.6"
$hashFile = Join-Path $hashcatDir "target_hash.txt"
$outFile = Join-Path $hashcatDir "found_pin.txt"
$cliExe = Join-Path $PSScriptRoot "target\release\imancraft-cli.exe"

if (Test-Path $outFile) { Remove-Item $outFile -Force }

Set-Location $hashcatDir

& .\hashcat.exe -m 16200 -a 3 target_hash.txt "?d?d?d?d?d?d?d?d" -w 3 --status --status-timer 5 --outfile found_pin.txt --outfile-format 2

Set-Location $PSScriptRoot

if (Test-Path $outFile) {
    $pin = (Get-Content $outFile -Raw).Trim()
    if ($pin) {
        Write-Host ""
        Write-Host "===============================================================================" -ForegroundColor Green
        Write-Host "  🎉 PASSWORD / PIN TROVATO: '$pin'" -ForegroundColor Green
        Write-Host "===============================================================================" -ForegroundColor Green
        Set-Content -Path "PIN_TROVATO.txt" -Value $pin
        Write-Host ""
        Write-Host "Decifratura automatica delle 6 note in corso..." -ForegroundColor Cyan
        & $cliExe unlock-notes "F:\DumpIOSPing1\Backup\00008110-000A648136D0A01E" $pin --output "NOTE_SBLOCCATE.txt"
        if (Test-Path "NOTE_SBLOCCATE.txt") {
            Start-Process notepad.exe "NOTE_SBLOCCATE.txt"
        }
    }
} else {
    Write-Host ""
    Write-Host "Scansione a 8 cifre completata. Nessuna corrispondenza trovata." -ForegroundColor Yellow
}

Write-Host ""
Write-Host "Premi un tasto per uscire..."
[Console]::ReadKey($true) | Out-Null
