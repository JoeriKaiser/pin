[CmdletBinding()]
param(
    [string]$Version = "latest",
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\pin",
    [string]$BaseUrl = "https://github.com/JoeriKaiser/pin/releases",
    [string]$AssetDirectory = "",
    [switch]$NoPathUpdate
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    throw "InstallDir is empty. Set -InstallDir or ensure LOCALAPPDATA is available."
}

$architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToUpperInvariant()
$target = switch ($architecture) {
    "X64" { "windows-amd64" }
    "ARM64" { "windows-arm64" }
    default { throw "Unsupported Windows architecture: $architecture" }
}

$assetName = "pin-$target.exe"
$checksumName = "$assetName.sha256"
$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("pin-install-" + [Guid]::NewGuid().ToString("N"))
$assetPath = Join-Path $tempDir $assetName
$checksumPath = Join-Path $tempDir $checksumName

try {
    New-Item -ItemType Directory -Path $tempDir -Force | Out-Null

    if ($AssetDirectory) {
        Copy-Item (Join-Path $AssetDirectory $assetName) $assetPath
        Copy-Item (Join-Path $AssetDirectory $checksumName) $checksumPath
    }
    else {
        $downloadRoot = if ($Version -eq "latest") {
            "$BaseUrl/latest/download"
        }
        else {
            "$BaseUrl/download/$Version"
        }

        Write-Host "==> Downloading $assetName..."
        Invoke-WebRequest -UseBasicParsing -Uri "$downloadRoot/$assetName" -OutFile $assetPath
        Invoke-WebRequest -UseBasicParsing -Uri "$downloadRoot/$checksumName" -OutFile $checksumPath
    }

    $expected = ((Get-Content -Raw $checksumPath) -split '\s+')[0].ToLowerInvariant()
    $actual = (Get-FileHash -Algorithm SHA256 $assetPath).Hash.ToLowerInvariant()
    if (-not $expected -or $expected -ne $actual) {
        throw "Checksum verification failed for $assetName."
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    $destination = Join-Path $InstallDir "pin.exe"
    Copy-Item $assetPath $destination -Force

    if (-not $NoPathUpdate) {
        $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
        $entries = @($userPath -split ";" | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
        if ($entries -notcontains $InstallDir) {
            $newPath = (@($entries) + $InstallDir) -join ";"
            [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
            Write-Host "Added $InstallDir to your user PATH. Open a new terminal to use it."
        }
    }

    Write-Host "Installed pin to $destination"
    & $destination --version
}
finally {
    if (Test-Path $tempDir) {
        Remove-Item -Recurse -Force $tempDir
    }
}
