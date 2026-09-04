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
            throw "install.ps1: Windows ARM64 builds are not published yet. Use an x64 machine or the GitHub Release."
        }
    }

    if ($Is64BitOperatingSystem) {
        return "windows-x86_64"
    }

    $label = if ($arch) { $arch } else { "unknown" }
    throw "install.ps1: unsupported architecture: $label"
}

$SoftwareUrl = if ($env:CORTEX_SOFTWARE_URL) { $env:CORTEX_SOFTWARE_URL.TrimEnd("/") } else { "https://software.cortex.foundation" }
$Channel = if ($env:CORTEX_CHANNEL) { $env:CORTEX_CHANNEL } else { "stable" }
$Prefix = if ($env:CORTEX_INSTALL_DIR) { $env:CORTEX_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Cortex" }
$BinDir = Join-Path $Prefix "bin"
$PinnedVersion = if ($env:CORTEX_VERSION) { $env:CORTEX_VERSION.TrimStart("v") } else { $null }

if ($Channel -notin @("stable", "beta", "nightly")) {
    throw "install.ps1: invalid CORTEX_CHANNEL='$Channel' (use stable, beta, or nightly)"
}

$processorArchitecture = $env:PROCESSOR_ARCHITECTURE
if ($env:PROCESSOR_ARCHITEW6432) {
    $processorArchitecture = $env:PROCESSOR_ARCHITEW6432
}

$Platform = Resolve-CortexInstallPlatform `
    -RuntimeOsArchitecture (Get-CortexRuntimeOsArchitecture) `
    -ProcessorArchitecture $processorArchitecture `
    -Is64BitOperatingSystem ([Environment]::Is64BitOperatingSystem)

function Get-Json($Url) {
    return Invoke-RestMethod -Uri $Url -Method Get
}

function Try-GetJson($Url) {
    try {
        return Invoke-RestMethod -Uri $Url -Method Get
    } catch {
        return $null
    }
}

Write-Host "Cortex CLI installer"
Write-Host "  host:     $SoftwareUrl"
Write-Host "  platform: $Platform"
Write-Host "  prefix:   $Prefix"

$release = $null
$version = $PinnedVersion

if ($version) {
    $release = Try-GetJson "$SoftwareUrl/releases/$version.json"
    if (-not $release) {
        $release = Try-GetJson "$SoftwareUrl/v1/releases/$version.json"
    }
    if (-not $release) {
        throw "install.ps1: could not fetch release metadata for $version from $SoftwareUrl"
    }
} else {
    $manifest = Try-GetJson "$SoftwareUrl/releases/manifest.json"
    if (-not $manifest) {
        $manifest = Try-GetJson "$SoftwareUrl/v1/releases/manifest.json"
    }
    if (-not $manifest) {
        throw "install.ps1: could not fetch $SoftwareUrl/releases/manifest.json"
    }
    $release = $manifest.$Channel
    if (-not $release) {
        throw "install.ps1: no $Channel release in manifest"
    }
    $version = $release.version
}

$asset = $release.assets.$Platform
if (-not $asset) {
    throw "install.ps1: no asset for platform $Platform in version $version"
}

$expectedSha = ([string]$asset.sha256).Trim().ToLowerInvariant()
if (-not $expectedSha) {
    throw "install.ps1: release JSON missing sha256 for $Platform"
}

Write-Host "  version:  $version"
Write-Host "  download: $($asset.url)"

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("cortex-install-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tempRoot | Out-Null
try {
    $zipPath = Join-Path $tempRoot "cortex.zip"
    Invoke-WebRequest -Uri $asset.url -OutFile $zipPath -UseBasicParsing

    $actualSha = (Get-FileHash -Path $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualSha -ne $expectedSha) {
        throw "install.ps1: SHA-256 mismatch for cortex.zip: expected $expectedSha, got $actualSha"
    }
    Write-Host "  checksum: ok"

    $extractDir = Join-Path $tempRoot "extract"
    Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force

    $binary = Get-ChildItem -Path $extractDir -Recurse -File |
        Where-Object { $_.Name -in @("Cortex.exe", "cortex.exe") } |
        Select-Object -First 1
    if (-not $binary) {
        throw "install.ps1: archive did not contain Cortex.exe"
    }

    New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
    $dest = Join-Path $BinDir "Cortex.exe"
    Copy-Item -Path $binary.FullName -Destination $dest -Force

    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $pathParts = @()
    if ($userPath) {
        $pathParts = $userPath.Split(";", [System.StringSplitOptions]::RemoveEmptyEntries)
    }
    if ($pathParts -notcontains $BinDir) {
        $newPath = if ($userPath) { "$userPath;$BinDir" } else { $BinDir }
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        $env:Path = "$env:Path;$BinDir"
        Write-Host "Added $BinDir to the user PATH."
    }

    Write-Host "Installed Cortex CLI v$version to $dest"
    Write-Host "Restart the terminal, then run: cortex --version"
} finally {
    Remove-Item -Recurse -Force $tempRoot -ErrorAction SilentlyContinue
}
