$ErrorActionPreference = "Stop"

$ScriptVersion = "0.1.20"
$RepoSlug = "OpenVulcan/vulcan-local-db"
$RawBaseUrl = "https://raw.githubusercontent.com/$RepoSlug/main/scripts"
$GlobalHome = Join-Path $HOME ".vulcan\vldb"
$GlobalConfig = Join-Path $GlobalHome "config.json"
$InstallDir = $null
$ManagerScriptVersion = $ScriptVersion

try {
    [Net.ServicePointManager]::SecurityProtocol = `
        [Net.SecurityProtocolType]::Tls12 -bor `
        [Net.SecurityProtocolType]::Tls11 -bor `
        [Net.SecurityProtocolType]::Tls
} catch {
}

function Write-ColorLine {
    param(
        [string]$Message,
        [ConsoleColor]$Color = [ConsoleColor]::Gray
    )

    try {
        Write-Host $Message -ForegroundColor $Color
    } catch {
        Write-Host $Message
    }
}

function Write-Info {
    param([string]$Message)
    Write-ColorLine -Message $Message -Color Cyan
}

function Write-Step {
    param([string]$Message)
    Write-ColorLine -Message ("[Step] " + $Message) -Color Yellow
}

function Write-Panel {
    param(
        [string]$Title,
        [ConsoleColor]$BorderColor = [ConsoleColor]::DarkCyan,
        [ConsoleColor]$TitleColor = [ConsoleColor]::Magenta
    )

    Write-ColorLine -Message "====================================" -Color $BorderColor
    Write-ColorLine -Message $Title -Color $TitleColor
    Write-ColorLine -Message "====================================" -Color $BorderColor
}

function Show-Banner {
    Write-Panel -Title "VulcanLocalDB Setup"
    Write-Info "The installer now installs only the manager."
}

function Read-Default {
    param([string]$PromptText, [string]$DefaultValue)
    $prompt = "$PromptText [$DefaultValue]"
    $value = Read-Host $prompt
    if ([string]::IsNullOrWhiteSpace($value)) { return $DefaultValue }
    return $value
}

function Confirm-Choice {
    param([string]$PromptText, [string]$DefaultValue = "Y")

    while ($true) {
        $prompt = "$PromptText [$DefaultValue]"
        $value = Read-Host $prompt
        if ([string]::IsNullOrWhiteSpace($value)) { $value = $DefaultValue }

        switch -Regex ($value) {
            "^[Yy]$" { return $true }
            "^[Nn]$" { return $false }
            default { Write-Info "Please input Y or N." }
        }
    }
}

function Normalize-Version {
    param([string]$Value)

    if ([string]::IsNullOrWhiteSpace($Value)) { return $null }

    $normalized = $Value.Trim()
    if ($normalized.StartsWith("v")) {
        $normalized = $normalized.Substring(1)
    }

    return $normalized
}

function Compare-VersionStrings {
    param([string]$Left, [string]$Right)

    $leftValue = Normalize-Version $Left
    $rightValue = Normalize-Version $Right

    if (-not $leftValue -and -not $rightValue) { return 0 }
    if (-not $leftValue) { return -1 }
    if (-not $rightValue) { return 1 }

    try {
        return ([version]$leftValue).CompareTo([version]$rightValue)
    } catch {
        return 0
    }
}

function Get-DefaultInstallDir {
    if (-not [string]::IsNullOrWhiteSpace($env:APPDATA)) {
        return (Join-Path $env:APPDATA "VulcanLocalDB")
    }

    if (-not [string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        return (Join-Path $env:LOCALAPPDATA "VulcanLocalDB")
    }

    return (Join-Path $HOME "AppData\Roaming\VulcanLocalDB")
}

function Get-PreferredInstallDir {
    $existingInstallDir = Get-ExistingInstallDir
    if ($existingInstallDir) {
        return $existingInstallDir
    }

    return (Get-DefaultInstallDir)
}

function Read-Config {
    if (-not (Test-Path $script:GlobalConfig)) {
        return $null
    }

    try {
        return Get-Content $script:GlobalConfig -Raw | ConvertFrom-Json
    } catch {
        return $null
    }
}

function Get-ExistingInstallDir {
    $candidates = @()
    $config = Read-Config

    if ($config -and $config.install_dir) {
        $candidates += [string]$config.install_dir
    }

    if (-not [string]::IsNullOrWhiteSpace($env:VULCANLOCALDB_HOME)) {
        $candidates += $env:VULCANLOCALDB_HOME
    }

    $candidates += (Get-DefaultInstallDir)

    foreach ($candidate in ($candidates | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Unique)) {
        $managerPath = Join-Path $candidate "bin\vldb.ps1"
        if (Test-Path $managerPath) {
            return [System.IO.Path]::GetFullPath($candidate)
        }
    }

    return $null
}

function Test-ValidInstallDir {
    param([string]$PathValue)

    if ([string]::IsNullOrWhiteSpace($PathValue)) { return $false }
    if (-not [System.IO.Path]::IsPathRooted($PathValue)) { return $false }

    try {
        [System.IO.Path]::GetFullPath($PathValue) | Out-Null
        return $true
    } catch {
        return $false
    }
}

function Choose-InstallDir {
    $defaultDir = Get-PreferredInstallDir

    while ($true) {
        $candidate = Read-Default "Installation directory" $defaultDir
        if (-not (Test-ValidInstallDir $candidate)) {
            Write-Info "Please use a legal absolute path."
            continue
        }

        if ((Test-Path $candidate) -and -not (Test-Path $candidate -PathType Container)) {
            Write-Info "The selected path already exists and is not a directory."
            continue
        }

        try {
            New-Item -ItemType Directory -Force -Path $candidate | Out-Null
        } catch {
            Write-Info "The installer cannot create or access this directory."
            continue
        }

        Write-Info "Install to: $candidate"
        if (Confirm-Choice "Confirm this installation directory?" "Y") {
            $script:InstallDir = [System.IO.Path]::GetFullPath($candidate)
            return
        }
    }
}

function Invoke-TextDownload {
    param([string]$Url)

    try {
        return (Invoke-WebRequest -UseBasicParsing -Uri $Url).Content
    } catch {
        return $null
    }
}

function Get-RemoteScriptVersion {
    param([string]$ScriptName, [string]$Pattern)

    $content = Invoke-TextDownload "$RawBaseUrl/$ScriptName"
    if (-not $content) { return $null }

    $match = [regex]::Match($content, $Pattern)
    if ($match.Success) {
        return $match.Groups[1].Value
    }

    return $null
}

function Show-UpdateNotice {
    $remoteScriptVersion = Get-RemoteScriptVersion -ScriptName "install.ps1" -Pattern '\$ScriptVersion\s*=\s*"([^"]+)"'

    if ($remoteScriptVersion -and (Compare-VersionStrings $remoteScriptVersion $ScriptVersion) -gt 0) {
        Write-Info "A newer installer script is available: $remoteScriptVersion (current: $ScriptVersion)."
    } else {
        Write-Info "Installer script version: $ScriptVersion"
    }
}

function Download-FileWithProgress {
    param(
        [string]$Url,
        [string]$OutFile,
        [string]$Label
    )

    Write-Info "Downloading $Label"
    Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $OutFile
}

function Get-ScriptVersionFromFile {
    param([string]$Path)

    if (-not (Test-Path $Path)) { return $null }

    $content = Get-Content $Path -Raw
    $match = [regex]::Match($content, '\$ScriptVersion\s*=\s*"([^"]+)"')
    if ($match.Success) {
        return $match.Groups[1].Value
    }

    return $null
}

function Install-ManagerScript {
    $sourcePath = $PSCommandPath
    $sourceDir = $null
    $binDir = Join-Path $script:InstallDir "bin"
    $managerScript = Join-Path $binDir "vldb.ps1"

    Write-Step "Installing manager script"
    New-Item -ItemType Directory -Force -Path $binDir | Out-Null

    if (-not [string]::IsNullOrWhiteSpace($sourcePath)) {
        $sourceDir = Split-Path -Parent $sourcePath
    }

    if ($sourceDir -and (Test-Path (Join-Path $sourceDir "vldb.ps1"))) {
        Write-Step "Copying bundled manager script"
        Copy-Item (Join-Path $sourceDir "vldb.ps1") $managerScript -Force
    } else {
        Write-Step "Downloading manager script from GitHub"
        Download-FileWithProgress -Url "$RawBaseUrl/vldb.ps1" -OutFile $managerScript -Label "vldb.ps1"
    }

    $detectedVersion = Get-ScriptVersionFromFile $managerScript
    if ($detectedVersion) {
        $script:ManagerScriptVersion = $detectedVersion
    }
}

function Write-GlobalConfig {
    $existingConfig = Read-Config
    $lanceRoot = if ($existingConfig -and $existingConfig.lancedb_root) {
        [string]$existingConfig.lancedb_root
    } else {
        Join-Path $script:GlobalHome "lancedb"
    }
    $duckRoot = if ($existingConfig -and $existingConfig.duckdb_root) {
        [string]$existingConfig.duckdb_root
    } else {
        Join-Path $script:GlobalHome "duckdb"
    }
    $releaseTag = if ($existingConfig -and $existingConfig.release_tag) {
        [string]$existingConfig.release_tag
    } else {
        $null
    }
    $initialized = $false
    if ($existingConfig -and $null -ne $existingConfig.initialized) {
        $initialized = [bool]$existingConfig.initialized
    }

    New-Item -ItemType Directory -Force -Path $script:GlobalHome | Out-Null
    @{
        language = if ($existingConfig -and $existingConfig.language) { [string]$existingConfig.language } else { "en" }
        install_dir = $script:InstallDir
        release_tag = $releaseTag
        script_version = $script:ManagerScriptVersion
        lancedb_root = $lanceRoot
        duckdb_root = $duckRoot
        initialized = $initialized
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 $script:GlobalConfig
}

function Refresh-CurrentSessionEnvironment {
    $combinedPath = @()

    foreach ($scope in @("Machine", "User")) {
        $scopePath = [Environment]::GetEnvironmentVariable("Path", $scope)
        if ($scopePath) {
            foreach ($entry in ($scopePath.Split(";") | Where-Object { $_ })) {
                if ($combinedPath -notcontains $entry) {
                    $combinedPath += $entry
                }
            }
        }
    }

    $env:Path = ($combinedPath -join ";")

    foreach ($name in @("VULCANLOCALDB_HOME", "VULCANLOCALDB_BIN")) {
        $value = [Environment]::GetEnvironmentVariable($name, "User")
        if ([string]::IsNullOrWhiteSpace($value)) {
            Remove-Item ("Env:" + $name) -ErrorAction SilentlyContinue
        } else {
            Set-Item ("Env:" + $name) $value
        }
    }
}

function Ensure-PathExports {
    $binDir = Join-Path $script:InstallDir "bin"
    Write-Step "Updating user PATH and environment variables"

    $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $parts = @()
    if ($currentPath) {
        $parts = $currentPath.Split(";") | Where-Object { $_ }
    }

    if ($parts -notcontains $binDir) {
        $parts += $binDir
        [Environment]::SetEnvironmentVariable("Path", ($parts -join ";"), "User")
    }

    [Environment]::SetEnvironmentVariable("VULCANLOCALDB_HOME", $script:InstallDir, "User")
    [Environment]::SetEnvironmentVariable("VULCANLOCALDB_BIN", $binDir, "User")
    Refresh-CurrentSessionEnvironment
}

function Invoke-InstalledManagerIfPresent {
    $existingInstallDir = Get-ExistingInstallDir
    if (-not $existingInstallDir) {
        return $false
    }

    $managerPath = Join-Path $existingInstallDir "bin\vldb.ps1"
    if (-not (Test-Path $managerPath)) {
        return $false
    }

    Write-Info "An existing VulcanLocalDB installation was detected at $existingInstallDir."
    Write-Info "Launching the local manager script."
    try {
        & $managerPath -FromInstaller
        return $true
    } catch {
        Write-Info "The existing manager could not start. Reinstalling the manager now."
        return $false
    }
}

function Launch-InstalledManager {
    $managerPath = Join-Path $script:InstallDir "bin\vldb.ps1"
    if (-not (Test-Path $managerPath)) {
        throw "Manager script was not installed successfully."
    }

    Write-Step "Starting manager"
    & $managerPath -FromInstaller
}

function Main {
    Show-Banner
    Show-UpdateNotice

    if (Invoke-InstalledManagerIfPresent) {
        return
    }

    Choose-InstallDir
    Install-ManagerScript
    Write-GlobalConfig
    Ensure-PathExports
    Launch-InstalledManager
}

Main
