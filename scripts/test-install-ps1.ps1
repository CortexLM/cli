# Guard scripts/install.ps1 architecture detection.
#
# Extracts Resolve-CortexInstallPlatform / Get-CortexRuntimeOsArchitecture from
# the installer AST so this test never downloads, never calls ::OSArchitecture
# through StrictMode, and does not execute the install body.

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$InstallScript = Join-Path $RepoRoot "scripts/install.ps1"
if (-not (Test-Path -LiteralPath $InstallScript)) {
    throw "test-install-ps1: missing $InstallScript"
}

$source = Get-Content -LiteralPath $InstallScript -Raw
if ($source -match '::OSArchitecture') {
    throw "test-install-ps1: install.ps1 must not access OSArchitecture via :: (StrictMode / Windows PowerShell 5.1)"
}
if ($source -notmatch 'Set-StrictMode') {
    throw "test-install-ps1: install.ps1 dropped Set-StrictMode"
}
if ($source -notmatch 'Windows ARM64 builds are not published yet') {
    throw "test-install-ps1: install.ps1 dropped the ARM64 error message"
}
if ($source -notmatch 'SHA-256 mismatch') {
    throw "test-install-ps1: install.ps1 dropped checksum verification"
}

$tokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile(
    $InstallScript,
    [ref]$tokens,
    [ref]$parseErrors
)
if ($parseErrors -and $parseErrors.Count -gt 0) {
    throw "test-install-ps1: parse errors:`n$($parseErrors | ForEach-Object { $_.ToString() } | Out-String)"
}

$functionAsts = $ast.FindAll({
        param($node)
        $node -is [System.Management.Automation.Language.FunctionDefinitionAst]
    }, $true)

foreach ($name in @("Resolve-CortexInstallPlatform", "Get-CortexRuntimeOsArchitecture", "Expand-CortexBinary")) {
    $fn = $functionAsts | Where-Object { $_.Name -eq $name } | Select-Object -First 1
    if ($null -eq $fn) {
        throw "test-install-ps1: function $name not found in install.ps1"
    }
    . ([scriptblock]::Create($fn.Extent.Text))
}

function Assert-Platform {
    param(
        [string]$RuntimeOsArchitecture,
        [string]$ProcessorArchitecture,
        [bool]$Is64BitOperatingSystem,
        [string]$Expected
    )
    $got = Resolve-CortexInstallPlatform `
        -RuntimeOsArchitecture $RuntimeOsArchitecture `
        -ProcessorArchitecture $ProcessorArchitecture `
        -Is64BitOperatingSystem $Is64BitOperatingSystem
    if ($got -ne $Expected) {
        throw "test-install-ps1: expected $Expected, got $got (runtime='$RuntimeOsArchitecture' proc='$ProcessorArchitecture' wow64=$Is64BitOperatingSystem)"
    }
}

function Assert-ThrowsLike {
    param(
        [scriptblock]$Script,
        [string]$Pattern,
        [string]$Label
    )
    $threw = $false
    try {
        & $Script
    } catch {
        $threw = $true
        $message = [string]$_.Exception.Message
        if ($message -notmatch $Pattern) {
            throw "test-install-ps1: $Label threw unexpected message: $message"
        }
    }
    if (-not $threw) {
        throw "test-install-ps1: $Label should have thrown"
    }
}

Assert-Platform -RuntimeOsArchitecture "X64" -ProcessorArchitecture "" -Is64BitOperatingSystem $false -Expected "windows-x86_64"
Assert-Platform -RuntimeOsArchitecture "" -ProcessorArchitecture "AMD64" -Is64BitOperatingSystem $false -Expected "windows-x86_64"
Assert-Platform -RuntimeOsArchitecture "" -ProcessorArchitecture "amd64" -Is64BitOperatingSystem $false -Expected "windows-x86_64"
Assert-ThrowsLike -Label "ambiguous 64-bit architecture" -Pattern "unsupported architecture" -Script {
    Resolve-CortexInstallPlatform -RuntimeOsArchitecture "" -ProcessorArchitecture "x86" -Is64BitOperatingSystem $true
}
Assert-ThrowsLike -Label "unknown 64-bit architecture" -Pattern "unsupported architecture" -Script {
    Resolve-CortexInstallPlatform -RuntimeOsArchitecture "" -ProcessorArchitecture "" -Is64BitOperatingSystem $true
}

Assert-ThrowsLike -Label "Arm64 runtime" -Pattern "Windows ARM64 builds are not published yet" -Script {
    Resolve-CortexInstallPlatform -RuntimeOsArchitecture "Arm64" -ProcessorArchitecture "" -Is64BitOperatingSystem $true
}
Assert-ThrowsLike -Label "ARM64 PROCESSOR_ARCHITECTURE" -Pattern "Windows ARM64 builds are not published yet" -Script {
    Resolve-CortexInstallPlatform -RuntimeOsArchitecture "" -ProcessorArchitecture "ARM64" -Is64BitOperatingSystem $true
}
Assert-ThrowsLike -Label "32-bit x86" -Pattern "unsupported architecture" -Script {
    Resolve-CortexInstallPlatform -RuntimeOsArchitecture "" -ProcessorArchitecture "x86" -Is64BitOperatingSystem $false
}

# Must not throw PropertyNotFoundStrict even when the host has (or lacks) the property.
Set-StrictMode -Version Latest
$null = Get-CortexRuntimeOsArchitecture

# Same probe the installer uses: missing static property + StrictMode + Stop.
Add-Type -TypeDefinition @"
public static class CortexInstallArchProbe {
    public static string Other { get { return "x"; } }
}
"@
$probeType = [CortexInstallArchProbe]
$missing = Get-Member -InputObject $probeType -MemberType Property -Name OSArchitecture -Static -ErrorAction SilentlyContinue
if ($null -ne $missing) {
    throw "test-install-ps1: expected OSArchitecture to be absent on CortexInstallArchProbe"
}
$flags = [System.Reflection.BindingFlags]::Public -bor [System.Reflection.BindingFlags]::Static
$missingProperty = $probeType.GetProperty("OSArchitecture", $flags)
if ($null -ne $missingProperty) {
    throw "test-install-ps1: GetProperty should return null for a missing static property"
}

$strictThrew = $false
try {
    $null = [CortexInstallArchProbe]::OSArchitecture
} catch {
    $strictThrew = $true
    $detail = [string]$_.FullyQualifiedErrorId + " " + [string]$_.Exception.Message
    if ($detail -notmatch 'PropertyNotFoundStrict' -and $detail -notmatch 'OSArchitecture') {
        throw "test-install-ps1: unexpected error probing missing static property: $detail"
    }
}
if (-not $strictThrew) {
    throw "test-install-ps1: StrictMode should reject [CortexInstallArchProbe]::OSArchitecture"
}

# Exercise extraction with local zip fixtures; never execute the fixture bytes.
Add-Type -AssemblyName System.IO.Compression.FileSystem
$fixtureRoot = Join-Path ([IO.Path]::GetTempPath()) ('cortex-installer-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixtureRoot | Out-Null
try {
    foreach ($name in @('../Cortex.exe', '/Cortex.exe', 'nested/Cortex.exe', 'other.exe', 'Cortex.exe')) {
        $zipPath = Join-Path $fixtureRoot 'fixture.zip'
        $destination = Join-Path $fixtureRoot 'extracted.exe'
        $zip = [IO.Compression.ZipFile]::Open($zipPath, [IO.Compression.ZipArchiveMode]::Create)
        try {
            $entry = $zip.CreateEntry($name)
            $writer = New-Object IO.StreamWriter($entry.Open())
            try { $writer.Write('fixture bytes, not an executable') } finally { $writer.Dispose() }
        } finally { $zip.Dispose() }
        if ($name -ceq 'Cortex.exe') {
            Expand-CortexBinary $zipPath $destination
            if ((Get-Content -LiteralPath $destination -Raw) -cne 'fixture bytes, not an executable') {
                throw 'test-install-ps1: extracted binary contents differ'
            }
            Remove-Item -LiteralPath $destination
        } else {
            Assert-ThrowsLike -Label "unsafe archive entry $name" -Pattern 'invalid archive entry' -Script {
                Expand-CortexBinary $zipPath $destination
            }
            if (Test-Path -LiteralPath $destination) { throw 'test-install-ps1: unsafe archive was extracted' }
        }
        Remove-Item -LiteralPath $zipPath
    }
} finally { Remove-Item -LiteralPath $fixtureRoot -Recurse -Force }

Write-Host "test-install-ps1: ok"
