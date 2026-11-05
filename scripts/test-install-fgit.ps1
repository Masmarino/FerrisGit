# Tests install-fgit.ps1 against a fake release on a local server, with a small fgit.exe compiled here, so that it needs
# neither a build nor a published release. Needs Windows PowerShell (for Add-Type -OutputAssembly) and python. It never
# touches the user's PATH: that part isn't exercised.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$scriptsDir = $PSScriptRoot
$port = if ($env:FGIT_TEST_PORT) { $env:FGIT_TEST_PORT } else { '18766' }
$root = Join-Path ([IO.Path]::GetTempPath()) "fgit-test-$([Guid]::NewGuid())"
New-Item -ItemType Directory -Path $root | Out-Null
$script:failures = 0

function Check([string] $Name, [bool] $Passed) {
    if ($Passed) {
        Write-Host "ok: $Name"
    } else {
        Write-Host "FAILED: $Name"
        $script:failures++
    }
}

$architecture = $env:PROCESSOR_ARCHITEW6432
if (-not $architecture) { $architecture = $env:PROCESSOR_ARCHITECTURE }
$target = if ($architecture -eq 'ARM64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
$archive = "fgit-$target.zip"

function New-FakeRelease([string] $Path, [string] $Version) {
    $dir = Join-Path $root "releases\$Path"
    $pack = Join-Path $root "pack-$Version"
    New-Item -ItemType Directory -Force -Path $dir, $pack | Out-Null
    $source = "class P { static void Main() { System.Console.WriteLine(""fgit $Version""); } }"
    Add-Type -TypeDefinition $source -OutputAssembly (Join-Path $pack 'fgit.exe') -OutputType ConsoleApplication
    Compress-Archive -Path (Join-Path $pack 'fgit.exe') -DestinationPath (Join-Path $dir $archive)
    $hash = (Get-FileHash -LiteralPath (Join-Path $dir $archive) -Algorithm SHA256).Hash.ToLower()
    # With Unix line endings and lower case, as sha256sum writes them on the CI's Linux runner.
    [IO.File]::WriteAllText((Join-Path $dir 'fgit-checksums.txt'), "$hash  $archive`n")
}

New-FakeRelease 'latest\download' '2.0.0'
New-FakeRelease 'download\v1.0.0' '1.0.0'
New-FakeRelease 'download\v6.6.6' '6.6.6'
Add-Content -LiteralPath (Join-Path $root "releases\download\v6.6.6\$archive") -Value 'tampered'

$python = (Get-Command python3, python -ErrorAction SilentlyContinue | Where-Object { $_.Source -notlike '*WindowsApps*' } | Select-Object -First 1).Source
$server = Start-Process -FilePath $python -ArgumentList '-m', 'http.server', $port, '--bind', '127.0.0.1' -WorkingDirectory $root -PassThru -WindowStyle Hidden

$installDir = Join-Path $root 'bin'
$installed = Join-Path $installDir 'fgit.exe'
$env:FGIT_BASE_URL = "http://127.0.0.1:$port/releases"
$env:FGIT_INSTALL_DIR = $installDir
$env:FGIT_NO_MODIFY_PATH = '1'

# In a child process, like `irm | iex` from a fresh terminal. Returns its exit code, with its output in $script:output.
function Invoke-Install([string] $Version) {
    $env:FGIT_VERSION = $Version
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $script:output = (& powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $scriptsDir 'install-fgit.ps1') 2>&1 | Out-String)
    $code = $LASTEXITCODE
    $ErrorActionPreference = $previous
    return $code
}

try {
    $ready = $false
    foreach ($attempt in 1..50) {
        try {
            Invoke-WebRequest -UseBasicParsing -Uri "$env:FGIT_BASE_URL/latest/download/fgit-checksums.txt" | Out-Null
            $ready = $true
            break
        } catch {
            Start-Sleep -Milliseconds 100
        }
    }
    if (-not $ready) { throw "the test server didn't start" }

    $pathBefore = (Get-Item -LiteralPath 'HKCU:\Environment').GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)

    $code = Invoke-Install ''
    Check 'the latest release installs' (($code -eq 0) -and ((& $installed) -eq 'fgit 2.0.0'))
    Check 'only fgit.exe lands in the folder' ((@(Get-ChildItem -Force -LiteralPath $installDir).Name -join ',') -eq 'fgit.exe')

    $code = Invoke-Install '1.0.0'
    Check 'a version can be pinned, and running again replaces the binary' (($code -eq 0) -and ((& $installed) -eq 'fgit 1.0.0'))
    $code = Invoke-Install 'v1.0.0'
    Check 'a version also reads with its v' (($code -eq 0) -and ((& $installed) -eq 'fgit 1.0.0'))

    $code = Invoke-Install '6.6.6'
    Check 'a tampered archive is refused' (($code -ne 0) -and ($script:output -match 'checksum'))
    Check 'and leaves what was installed' ((& $installed) -eq 'fgit 1.0.0')

    $code = Invoke-Install '9.9.9'
    Check 'an unknown version is a clear error' (($code -ne 0) -and ($script:output -match 'check the version'))

    $pathAfter = (Get-Item -LiteralPath 'HKCU:\Environment').GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    Check 'FGIT_NO_MODIFY_PATH leaves the PATH alone' ($pathBefore -eq $pathAfter)
} finally {
    Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}

if ($script:failures -gt 0) {
    Write-Error "$($script:failures) check(s) failed"
    exit 1
}
Write-Host 'all checks passed'
# The last check leaves the exit code of an install meant to fail, which GitHub's runner would take for this script's.
exit 0
