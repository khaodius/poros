# Installs Poros on Windows from its GitHub releases: the latest release, or the version given as
# an argument. Downloads the setup program, checks it against the release's SHA256SUMS.txt and
# runs it silently. Poros installs for the current user, in %LOCALAPPDATA%\Poros, without admin
# rights. An installed copy is replaced in place with its settings and saved connections kept,
# and a running one is closed first.
#
#   irm https://raw.githubusercontent.com/khaodius/poros/main/scripts/install.ps1 | iex
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/khaodius/poros/main/scripts/install.ps1))) 0.4.9

param(
    [Parameter(Position = 0)]
    [string]$Version = ''
)

# Piped into Invoke-Expression this runs in the caller's own session, so the work happens in a
# child scope that leaves none of its variables or preferences behind, and failures throw rather
# than exit, which would close the window.
& {
    param([string]$RequestedVersion)

    $ErrorActionPreference = 'Stop'
    # Windows PowerShell redraws its progress bar for every chunk, which slows downloads severalfold.
    $ProgressPreference = 'SilentlyContinue'
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $releases = 'https://github.com/khaodius/poros/releases'
    # Release file names carry the version between this prefix and suffix.
    $prefix = 'Poros_'
    $suffix = '_x64-setup.exe'
    $uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Poros'

    if (-not [Environment]::Is64BitOperatingSystem) {
        throw 'Poros needs 64-bit Windows.'
    }
    $RequestedVersion = $RequestedVersion -replace '^v', ''
    if ($RequestedVersion -and $RequestedVersion -notmatch '^\d+\.\d+\.\d+([-+][0-9A-Za-z.-]+)?$') {
        throw "'$RequestedVersion' is not a version number such as 0.4.9."
    }

    $workDirectory = Join-Path ([IO.Path]::GetTempPath()) ('poros-install-' + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $workDirectory | Out-Null
    try {
        if ($RequestedVersion) {
            $checksumsUrl = "$releases/download/v$RequestedVersion/SHA256SUMS.txt"
        }
        else {
            $checksumsUrl = "$releases/latest/download/SHA256SUMS.txt"
        }
        $checksumsPath = Join-Path $workDirectory 'SHA256SUMS.txt'
        try {
            Invoke-WebRequest -Uri $checksumsUrl -OutFile $checksumsPath -UseBasicParsing
        }
        catch {
            $hint = if ($RequestedVersion) { " There may be no $RequestedVersion release." } else { '' }
            throw "Could not download ${checksumsUrl}: $($_.Exception.Message)$hint"
        }

        $expectedHash = $null
        $fileName = $null
        foreach ($line in Get-Content -Path $checksumsPath) {
            if ($line -match '^([0-9A-Fa-f]{64}) [ *](.+)$' -and $Matches[2].StartsWith($prefix) -and $Matches[2].EndsWith($suffix)) {
                $expectedHash = $Matches[1]
                $fileName = $Matches[2]
            }
        }
        if (-not $fileName) {
            throw "The release has no $prefix*$suffix setup program."
        }
        $releaseVersion = $fileName.Substring($prefix.Length, $fileName.Length - $prefix.Length - $suffix.Length)

        Write-Host "Downloading Poros $releaseVersion ($fileName)"
        $setupPath = Join-Path $workDirectory $fileName
        try {
            Invoke-WebRequest -Uri "$releases/download/v$releaseVersion/$fileName" -OutFile $setupPath -UseBasicParsing
        }
        catch {
            throw "Could not download ${fileName}: $($_.Exception.Message)"
        }
        if ((Get-FileHash -Path $setupPath -Algorithm SHA256).Hash -ne $expectedHash) {
            throw "$fileName does not match its SHA-256 checksum in SHA256SUMS.txt, so nothing was installed."
        }

        Write-Host "Installing Poros $releaseVersion"
        $setup = Start-Process -FilePath $setupPath -ArgumentList '/S' -Wait -PassThru
        if ($setup.ExitCode -ne 0) {
            throw "The setup program stopped with exit code $($setup.ExitCode)."
        }
        $installed = Get-ItemProperty -Path $uninstallKey -ErrorAction SilentlyContinue
        if (-not $installed -or $installed.DisplayVersion -ne $releaseVersion) {
            throw "The setup program finished, but Windows does not list Poros $releaseVersion as installed."
        }

        Write-Host "Poros $releaseVersion is installed in $($installed.InstallLocation.Trim('"')). Open it from the Start menu or the desktop shortcut."
    }
    finally {
        Remove-Item -Path $workDirectory -Recurse -Force -ErrorAction SilentlyContinue
    }
} $Version
