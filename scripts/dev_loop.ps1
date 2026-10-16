# Relay dev loop (D-096): try a new build on the owner's PC with no installer and no
# admin prompt, run the full diagnostics, and print where the results are.
#
#   powershell -ExecutionPolicy Bypass -File dev_loop.ps1 -Package <Relay-dev.zip> [-Only id ...]
#   powershell -ExecutionPolicy Bypass -File dev_loop.ps1 -Run [-Only id ...]   # test what is there
#   powershell -ExecutionPolicy Bypass -File dev_loop.ps1 -Restore             # back to installed Relay
#
# The dev copy lives in Downloads\Relay-dev\current (relay.exe and its python folder,
# the same layout the installer writes to Program Files). It shares the owner's
# settings, models and chats with the installed Relay. Only one Relay runs at a time,
# so the installed one is closed while the dev copy runs, and -Restore starts it again.
# A package may hold only part of the copy (just relay.exe, or just python\Lib\...): it
# is unpacked over what is there.
param(
    [string]$Package,
    [switch]$Run,
    [switch]$Restore,
    [string[]]$Only,
    [int]$WaitSeconds = 90
)
$ErrorActionPreference = 'Stop'
$downloads = Join-Path ([Environment]::GetFolderPath('UserProfile')) 'Downloads'
$dev = Join-Path $downloads 'Relay-dev'
$current = Join-Path $dev 'current'
$installed = Join-Path $env:ProgramFiles 'Relay'
$record = Join-Path $env:LOCALAPPDATA 'Relay\operator.json'

function Stop-Relay {
    # Every Relay window and the Python it started (operator service, relay-screen).
    $roots = @($current, $installed)
    Get-CimInstance Win32_Process |
        Where-Object { $p = $_.ExecutablePath; $p -and ($roots | Where-Object { $p.StartsWith($_ + '\') }) } |
        ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
    Start-Sleep -Milliseconds 800
}

function Start-Relay([string]$folder) {
    Remove-Item $record -ErrorAction SilentlyContinue
    Start-Process (Join-Path $folder 'relay.exe')
    $until = (Get-Date).AddSeconds($WaitSeconds)
    while (-not (Test-Path $record) -and (Get-Date) -lt $until) { Start-Sleep -Milliseconds 500 }
    return (Test-Path $record)
}

if ($Restore) {
    Stop-Relay
    $ok = Start-Relay $installed
    @{ ok = $ok; message = 'Installed Relay started again.' } | ConvertTo-Json -Compress
    exit ([int](-not $ok))
}

if ($Package) {
    Stop-Relay
    New-Item -ItemType Directory -Force $current | Out-Null
    Expand-Archive -Path $Package -DestinationPath $current -Force
}

if (-not (Test-Path (Join-Path $current 'relay.exe'))) {
    @{ ok = $false; message = "No dev copy in $current yet: pass -Package." } | ConvertTo-Json -Compress
    exit 2
}

$running = Get-CimInstance Win32_Process -Filter "Name = 'relay.exe'" |
    Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($current + '\') }
if (-not $running) {
    Stop-Relay
    if (-not (Start-Relay $current)) {
        @{ ok = $false; message = 'The dev copy started but its local AI service did not come up.' } |
            ConvertTo-Json -Compress
    }
}

$python = Join-Path $current 'python\python.exe'
$arguments = @('-m', 'relay_screen.server', 'diagnostics', '--json')
if ($Only) { $arguments += '--only'; $arguments += $Only }
$env:PYTHONUTF8 = '1'
$log = Join-Path $dev 'last-run.jsonl'
& $python @arguments | Tee-Object -FilePath $log | Select-String '"event": "(result|summary|unavailable)"' |
    ForEach-Object { $_.Line.Substring(0, [Math]::Min(400, $_.Line.Length)) }
$code = $LASTEXITCODE
$latest = Join-Path $downloads 'Relay-diagnostics\latest.json'
if (Test-Path $latest) { Get-Content $latest -Raw }
exit $code
