# Installs fgit on Windows, from the binaries attached to a GitHub release:
#
#   irm https://raw.githubusercontent.com/Masmarino/FerrisGit/main/scripts/install-fgit.ps1 | iex
#
# FGIT_VERSION picks a release (1.2.3, the latest by default) and FGIT_INSTALL_DIR the folder (%LOCALAPPDATA%\Programs\fgit
# by default), which is added to the user's PATH unless FGIT_NO_MODIFY_PATH is set. Running it again upgrades. It needs no
# administrator rights.

# In functions and called last, so that a download cut short runs nothing. Errors throw rather than exit: with `iex`, an
# exit would close the terminal of whoever is running this.
function Get-FgitTarget {
    $architecture = $env:PROCESSOR_ARCHITEW6432
    if (-not $architecture) { $architecture = $env:PROCESSOR_ARCHITECTURE }
    switch ($architecture) {
        'AMD64' { return 'x86_64-pc-windows-msvc' }
        'ARM64' { return 'aarch64-pc-windows-msvc' }
        default { throw "unsupported processor $architecture" }
    }
}

# Through the registry, not [Environment]: that one expands %USERPROFILE%-style entries for good when it writes them back.
function Add-FgitToUserPath([string] $Directory) {
    $key = Get-Item -LiteralPath 'HKCU:\Environment'
    $current = $key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    if (($current -split ';') -contains $Directory) { return $false }
    $updated = if ($current) { "$($current.TrimEnd(';'));$Directory" } else { $Directory }
    Set-ItemProperty -LiteralPath 'HKCU:\Environment' -Name 'Path' -Value $updated -Type ExpandString
    # This write only serves to tell open programs that the environment changed, so that new terminals see the PATH.
    [Environment]::SetEnvironmentVariable('FGIT_INSTALL_REFRESH', '1', 'User')
    [Environment]::SetEnvironmentVariable('FGIT_INSTALL_REFRESH', $null, 'User')
    return $true
}

function Install-Fgit {
    $ErrorActionPreference = 'Stop'
    # Windows PowerShell's default progress bar makes downloads many times slower.
    $ProgressPreference = 'SilentlyContinue'
    # Windows PowerShell 5.1 can default to protocols GitHub refuses.
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $target = Get-FgitTarget
    $base = if ($env:FGIT_BASE_URL) { $env:FGIT_BASE_URL } else { 'https://github.com/Masmarino/FerrisGit/releases' }
    if ($env:FGIT_VERSION) {
        $version = $env:FGIT_VERSION -replace '^v', ''
        $release = "$base/download/v$version"
        $wanted = "version $version"
    } else {
        $release = "$base/latest/download"
        $wanted = 'the latest release'
    }
    $installDir = if ($env:FGIT_INSTALL_DIR) { $env:FGIT_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\fgit' }
    $archive = "fgit-$target.zip"

    $work = Join-Path ([IO.Path]::GetTempPath()) "fgit-$([Guid]::NewGuid())"
    New-Item -ItemType Directory -Path $work | Out-Null
    try {
        $archivePath = Join-Path $work $archive
        $checksumsPath = Join-Path $work 'checksums.txt'

        Write-Host "Downloading fgit for $target ($wanted)..."
        try {
            Invoke-WebRequest -UseBasicParsing -Uri "$release/$archive" -OutFile $archivePath
        } catch {
            throw "could not download $archive from ${wanted}: check the version, and that this release has fgit binaries"
        }
        try {
            Invoke-WebRequest -UseBasicParsing -Uri "$release/fgit-checksums.txt" -OutFile $checksumsPath
        } catch {
            throw "could not download the checksums of $wanted, so the download can't be checked"
        }

        $expected = $null
        foreach ($line in Get-Content -LiteralPath $checksumsPath) {
            $fields = $line -split '\s+'
            if ($fields.Count -ge 2 -and $fields[1].TrimStart('*') -eq $archive) { $expected = $fields[0] }
        }
        if (-not $expected) { throw "$archive isn't in the release's checksums" }
        $actual = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash
        if ($actual -ne $expected) { throw "the checksum of $archive doesn't match: nothing was installed" }

        $unpacked = Join-Path $work 'unpacked'
        Expand-Archive -LiteralPath $archivePath -DestinationPath $unpacked
        $downloaded = Join-Path $unpacked 'fgit.exe'
        if (-not (Test-Path -LiteralPath $downloaded)) { throw "$archive has no fgit.exe" }
        & $downloaded --version | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "the downloaded fgit doesn't run on this machine" }

        New-Item -ItemType Directory -Force -Path $installDir | Out-Null
        $installed = Join-Path $installDir 'fgit.exe'
        try {
            Copy-Item -LiteralPath $downloaded -Destination $installed -Force
        } catch {
            throw "could not replace $installed. Is fgit running? Close it and try again"
        }
    } finally {
        Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    }

    Write-Host "Installed $(& $installed --version) in $installDir"

    if ($env:FGIT_NO_MODIFY_PATH) {
        if (($env:Path -split ';') -notcontains $installDir) { Write-Host "$installDir isn't on your PATH." }
        return
    }
    if (Add-FgitToUserPath $installDir) {
        Write-Host "Added $installDir to your PATH."
    }
    if (($env:Path -split ';') -notcontains $installDir) { $env:Path = "$env:Path;$installDir" }
    Write-Host 'Next: fgit auth login --hostname <your instance>'
}

Install-Fgit
