# Install Cortex CLI from https://software.cortex.foundation
#
# Usage:
#   irm https://software.cortex.foundation/install.ps1 | iex
#
# Optional environment:
#   CORTEX_VERSION       Pin a version (e.g. 0.1.2). Default: latest on the channel.
#   CORTEX_CHANNEL       stable (default), beta, or nightly
#   CORTEX_INSTALL_DIR   Prefix (default: $env:LOCALAPPDATA\Cortex). Binary in PREFIX\bin.
#   CORTEX_SOFTWARE_URL  Override the distribution host (testing only).
#
# Downloads the matching zip, verifies SHA-256, then installs Cortex.exe.
# Checksum verification is required; the script will not install an unverified file.

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# RuntimeInformation.OSArchitecture is missing on Windows PowerShell 5.1 / older
# .NET. Reading that static property under Set-StrictMode throws
# PropertyNotFoundStrict, so probe with Get-Member / GetProperty instead.
function Get-CortexRuntimeOsArchitecture {
    try {
        $riType = [System.Runtime.InteropServices.RuntimeInformation]
    } catch {
        return $null
    }
    if ($null -eq $riType) {
        return $null
    }

    $member = Get-Member -InputObject $riType -MemberType Property -Name OSArchitecture -Static -ErrorAction SilentlyContinue
    if ($null -eq $member) {
        return $null
    }

    $flags = [System.Reflection.BindingFlags]::Public -bor [System.Reflection.BindingFlags]::Static
    $property = $riType.GetProperty("OSArchitecture", $flags)
    if ($null -eq $property) {
        return $null
    }
    try {
        $value = $property.GetValue($null, $null)
    } catch {
        return $null
    }
    if ($null -eq $value) {
        return $null
    }
    return [string]$value
}

function Resolve-CortexInstallPlatform {
    param(
        [string]$RuntimeOsArchitecture,
        [string]$ProcessorArchitecture,
        [bool]$Is64BitOperatingSystem
    )

    $arch = $RuntimeOsArchitecture
    if (-not $arch) {
        $arch = $ProcessorArchitecture
    }

    switch -Regex ([string]$arch) {
        '^(?i:X64|AMD64)$' { return "windows-x86_64" }
        '^(?i:ARM64)$' {
            throw "install.ps1: Windows ARM64 builds are not published yet. Use a supported x64 Windows machine."
        }
    }


    $label = if ($arch) { $arch } else { "unknown" }
    throw "install.ps1: unsupported architecture: $label"
}

# Bounded same-origin downloads, with no redirect or remote HTTP fallback.
function Save-CortexDownload {
    param([string]$Url, [string]$Destination, [long]$Limit)
    $request = [System.Net.HttpWebRequest]::Create($Url)
    $request.AllowAutoRedirect = $false
    $request.Timeout = 10000
    $request.ReadWriteTimeout = 10000
    $response = $null
    $source = $null
    $output = $null
    try {
        $response = $request.GetResponse()
        if ([int]$response.StatusCode -ne 200 -or $response.ContentLength -gt $Limit) {
            throw "install.ps1: unexpected download status or size"
        }
        $source = $response.GetResponseStream()
        $output = [System.IO.File]::Create($Destination)
        $buffer = New-Object byte[] 65536
        $total = 0L
        $timer = [System.Diagnostics.Stopwatch]::StartNew()
        while (($count = $source.Read($buffer, 0, $buffer.Length)) -gt 0) {
            $total += $count
            if ($total -gt $Limit -or $timer.Elapsed.TotalSeconds -gt 120) {
                throw "install.ps1: download exceeds size/time limit"
            }
            $output.Write($buffer, 0, $count)
        }
    } finally {
        if ($null -ne $output) { $output.Dispose() }
        if ($null -ne $source) { $source.Dispose() }
        if ($null -ne $response) { $response.Dispose() }
    }
}

function Expand-CortexBinary {
    param([string]$Archive, [string]$Destination)
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        if ($zip.Entries.Count -ne 1) { throw "install.ps1: archive must contain only Cortex.exe" }
        $entry = $zip.Entries[0]
        if ($entry.FullName -cne 'Cortex.exe' -or $entry.Length -le 0 -or $entry.Length -gt 536870912) {
            throw "install.ps1: invalid archive entry"
        }
        # Reject Unix symlink/device entries as well as directory entries.
        $kind = ($entry.ExternalAttributes -shr 16) -band 61440
        if ($kind -ne 0 -and $kind -ne 32768) { throw "install.ps1: non-regular archive entry" }
        $source = $entry.Open()
        $output = $null
        try {
            $output = [IO.File]::Open($Destination, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write)
            $buffer = New-Object byte[] 65536
            $total = 0L
            while (($count = $source.Read($buffer, 0, $buffer.Length)) -gt 0) {
                $total += $count
                if ($total -gt $entry.Length -or $total -gt 536870912) {
                    throw "install.ps1: extracted size limit exceeded"
                }
                $output.Write($buffer, 0, $count)
            }
            if ($total -ne $entry.Length) { throw "install.ps1: extracted size mismatch" }
        } finally {
            if ($null -ne $output) { $output.Dispose() }
            $source.Dispose()
        }
    } finally { $zip.Dispose() }
}

function Test-CortexVersion {
    param([string]$Binary, [string]$Version, [string]$Scratch)
    $out = Join-Path $Scratch 'version.stdout'
    $err = Join-Path $Scratch 'version.stderr'
    $process = Start-Process -FilePath $Binary -ArgumentList '--version' -PassThru -NoNewWindow `
        -RedirectStandardOutput $out -RedirectStandardError $err
    try {
        if (-not $process.WaitForExit(15000)) {
            $process.Kill()
            throw "install.ps1: binary version check timed out"
        }
        $process.WaitForExit()
        if ((Get-Item -LiteralPath $out).Length -gt 65536) { throw "install.ps1: invalid version output" }
        $words = ((Get-Content -LiteralPath $out -Raw).Trim() -split '\s+')
        if ($process.ExitCode -ne 0 -or $words[-1] -cne $Version) {
            throw "install.ps1: binary version check failed"
        }
    } finally { $process.Dispose() }
}

# Windows-only; do not guess a platform from the process bitness.
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw "install.ps1: unsupported OS (Windows required)"
}
$SoftwareUrl = if ($env:CORTEX_SOFTWARE_URL) { $env:CORTEX_SOFTWARE_URL.TrimEnd('/') } else { 'https://software.cortex.foundation' }
$origin = [Uri]$SoftwareUrl
if (-not $origin.IsAbsoluteUri -or $origin.UserInfo -or $origin.Query -or $origin.Fragment -or $origin.AbsolutePath -ne '/' -or
    ($origin.Scheme -ne 'https' -and -not ($origin.Scheme -eq 'http' -and $origin.Host -in @('127.0.0.1', 'localhost', '[::1]')))) {
    throw "install.ps1: distribution URL must be an HTTPS origin (HTTP only for loopback tests)"
}
$Channel = if ($env:CORTEX_CHANNEL) { $env:CORTEX_CHANNEL } else { 'stable' }
if ($Channel -notin @('stable', 'beta', 'nightly')) { throw "install.ps1: invalid release channel" }
$Prefix = if ($env:CORTEX_INSTALL_DIR) { $env:CORTEX_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Cortex' }
$BinDir = Join-Path $Prefix 'bin'
$version = if ($env:CORTEX_VERSION) { $env:CORTEX_VERSION -replace '^v', '' } else { '' }
$versionPattern = '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$'
if ($version -and $version -cnotmatch $versionPattern) { throw "install.ps1: invalid version" }
$processorArchitecture = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
$Platform = Resolve-CortexInstallPlatform -RuntimeOsArchitecture (Get-CortexRuntimeOsArchitecture) `
    -ProcessorArchitecture $processorArchitecture -Is64BitOperatingSystem ([Environment]::Is64BitOperatingSystem)
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('cortex-install-' + [guid]::NewGuid().ToString('N'))
$stageDir = $null
New-Item -ItemType Directory -Path $tempRoot | Out-Null
try {
    $metadata = Join-Path $tempRoot 'release.json'
    $file = if ($version) { "$version.json" } else { 'manifest.json' }
    try { Save-CortexDownload "$SoftwareUrl/releases/$file" $metadata 2097152 }
    catch { Save-CortexDownload "$SoftwareUrl/v1/releases/$file" $metadata 2097152 }
    $data = Get-Content -LiteralPath $metadata -Raw | ConvertFrom-Json
    $release = if ($version) { $data } else { $data.$Channel }
    if (-not $release -or $release.version -cnotmatch $versionPattern -or
        ($version -and $release.version -cne $version) -or $release.channel -cne $Channel) {
        throw "install.ps1: release version/channel mismatch"
    }
    $version = $release.version
    $asset = $release.assets.$Platform
    $expectedUrl = "$SoftwareUrl/v1/assets/$Platform/$version/cortex.zip"
    if (-not $asset -or $asset.url -cne $expectedUrl -or $asset.sha256 -cnotmatch '^[a-fA-F0-9]{64}$' -or
        ($asset.size -isnot [int] -and $asset.size -isnot [long]) -or $asset.size -le 0 -or $asset.size -gt 536870912) {
        throw "install.ps1: invalid asset metadata"
    }
    $zipPath = Join-Path $tempRoot 'cortex.zip'
    Save-CortexDownload $asset.url $zipPath $asset.size
    if ((Get-Item -LiteralPath $zipPath).Length -ne $asset.size) { throw "install.ps1: archive size mismatch" }
    if ((Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash -ine $asset.sha256) {
        throw "install.ps1: SHA-256 mismatch"
    }
    New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
    $dest = Join-Path $BinDir 'Cortex.exe'
    $backup = Join-Path $BinDir 'Cortex.old.exe'
    foreach ($path in @($dest, $backup)) {
        if (Test-Path -LiteralPath $path) {
            $item = Get-Item -LiteralPath $path -Force
            if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
                throw "install.ps1: refusing non-regular installation target"
            }
        }
    }
    $stageDir = Join-Path $BinDir ('.cortex-install-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $stageDir | Out-Null
    $staged = Join-Path $stageDir 'Cortex.exe'
    Expand-CortexBinary $zipPath $staged
    Test-CortexVersion $staged $version $stageDir
    $hadPrevious = Test-Path -LiteralPath $dest
    if ($hadPrevious) { [IO.File]::Replace($staged, $dest, $backup) }
    else { [IO.File]::Move($staged, $dest) }
    try { Test-CortexVersion $dest $version $stageDir }
    catch {
        if ($hadPrevious) { [IO.File]::Replace($backup, $dest, $null) }
        else { Remove-Item -LiteralPath $dest }
        throw
    }
    # Do not overwrite unrelated agent.exe commands or edit the user's PATH.
    Write-Host "Installed Cortex CLI v$version to $dest"
    Write-Host "Add $BinDir to your user PATH, then run: cortex --version"
    if ($hadPrevious) { Write-Host "Previous binary retained at $backup" }
} finally {
    if ($stageDir) { Remove-Item -LiteralPath $stageDir -Recurse -Force -ErrorAction SilentlyContinue }
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}
