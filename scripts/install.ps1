$ErrorActionPreference = "Stop"

$ScriptVersion = "0.1.0"
$RepoSlug = "OpenVulcan/vulcan-local-db"
$RepoUrl = "https://github.com/OpenVulcan/vulcan-local-db"
$RawBaseUrl = "https://raw.githubusercontent.com/$RepoSlug/main/scripts"
$GlobalHome = Join-Path $HOME ".vulcan\vldg"
$GlobalConfig = Join-Path $GlobalHome "config.json"
$RunDir = Join-Path $GlobalHome "run"
$InstallDir = $null
$ReleaseTag = $null
$HostBind = "127.0.0.1"
$LanceDbPort = 50051
$DuckDbPort = 50052
$InstallMode = "full"
$WinSWVersion = "v2.12.0"
$ControllerScriptVersion = $ScriptVersion

try {
    [Net.ServicePointManager]::SecurityProtocol = `
        [Net.SecurityProtocolType]::Tls12 -bor `
        [Net.SecurityProtocolType]::Tls11 -bor `
        [Net.SecurityProtocolType]::Tls
} catch {
}

function Write-Info {
    param([string]$Message)
    Write-Host $Message
}

function Show-Banner {
    Write-Host "===================================="
    Write-Host "       VulcanLocalDB Setup"
    Write-Host "===================================="
    Write-Host "PowerShell 5.x build currently uses English only."
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

function Get-IsAdmin {
    $current = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($current)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Get-DefaultInstallDir {
    if (Get-IsAdmin) {
        return (Join-Path $env:ProgramFiles "VulcanLocalDB")
    }
    return (Join-Path $env:LOCALAPPDATA "Programs\VulcanLocalDB")
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

function Test-ValidPort {
    param([string]$Value)

    if ($Value -notmatch '^\d+$') { return $false }
    $port = [int]$Value
    return $port -ge 1 -and $port -le 65535
}

function Choose-InstallMode {
    Write-Host "1. Full install (services + controller)"
    Write-Host "2. Controller only"
    while ($true) {
        $choice = Read-Host "Select mode [1]"
        switch ($choice) {
            "" { $script:InstallMode = "full"; return }
            "1" { $script:InstallMode = "full"; return }
            "2" { $script:InstallMode = "controller-only"; return }
            default { Write-Info "Please input 1 or 2." }
        }
    }
}

function Choose-InstallDir {
    $defaultDir = Get-DefaultInstallDir

    while ($true) {
        $candidate = Read-Default "Installation directory" $defaultDir
        if (-not (Test-ValidInstallDir $candidate)) {
            Write-Info "Please use a legal absolute path."
            continue
        }

        $candidateExists = Test-Path $candidate
        $candidateIsDir = Test-Path $candidate -PathType Container
        if ($candidateExists -and -not $candidateIsDir) {
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

function Choose-NetworkSettings {
    while ($true) {
        $script:HostBind = Read-Default "Service bind IP" "127.0.0.1"
        if ([string]::IsNullOrWhiteSpace($script:HostBind)) {
            Write-Info "IP must not be empty."
            continue
        }

        $lancePortInput = Read-Default "LanceDB port" "50051"
        if (-not (Test-ValidPort $lancePortInput)) {
            Write-Info "Invalid LanceDB port."
            continue
        }
        $script:LanceDbPort = [int]$lancePortInput

        $duckPortInput = Read-Default "DuckDB port" "50052"
        if (-not (Test-ValidPort $duckPortInput)) {
            Write-Info "Invalid DuckDB port."
            continue
        }
        $script:DuckDbPort = [int]$duckPortInput

        if ($script:LanceDbPort -eq $script:DuckDbPort) {
            Write-Info "LanceDB and DuckDB must use different ports."
            continue
        }

        return
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

function Try-GetLatestReleaseTag {
    try {
        return (Invoke-RestMethod -Uri "https://api.github.com/repos/$RepoSlug/releases/latest").tag_name
    } catch {
        return $null
    }
}

function Show-UpdateNotice {
    $remoteScriptVersion = Get-RemoteScriptVersion -ScriptName "install.ps1" -Pattern '\$ScriptVersion\s*=\s*"([^"]+)"'
    $latestTag = Try-GetLatestReleaseTag

    if ($remoteScriptVersion -and (Compare-VersionStrings $remoteScriptVersion $ScriptVersion) -gt 0) {
        Write-Info "A newer installer script is available: $remoteScriptVersion (current: $ScriptVersion)."
    } else {
        Write-Info "Installer script version: $ScriptVersion"
    }

    if ($latestTag) {
        Write-Info "Latest release tag: $latestTag"
    }
}

function Get-LatestRelease {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$RepoSlug/releases/latest"
    if (-not $release.tag_name) {
        throw "Unable to resolve the latest release tag."
    }
    $script:ReleaseTag = $release.tag_name
    return $release
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

function Get-TargetTriple {
    $arch = $env:PROCESSOR_ARCHITECTURE
    switch ($arch) {
        "AMD64" { return "x86_64-pc-windows-msvc" }
        "ARM64" { return "aarch64-pc-windows-msvc" }
        default { throw "Unsupported Windows CPU architecture." }
    }
}

function Download-AssetPair {
    param(
        [string]$Service,
        [string]$Tag,
        [string]$Target,
        [string]$TempDir,
        [object]$Release
    )

    $archiveName = "$Service-$Tag-$Target.zip"
    $checksumName = "$archiveName.sha256"
    $archivePath = Join-Path $TempDir $archiveName
    $checksumPath = Join-Path $TempDir $checksumName
    $baseUrl = "$RepoUrl/releases/download/$Tag"

    if ($Release.assets.name -notcontains $archiveName) {
        throw "The current release does not provide $archiveName."
    }

    Download-FileWithProgress -Url "$baseUrl/$archiveName" -OutFile $archivePath -Label $archiveName
    Download-FileWithProgress -Url "$baseUrl/$checksumName" -OutFile $checksumPath -Label $checksumName

    $expected = (Get-Content $checksumPath -Raw).Split(" ")[0].Trim().ToLowerInvariant()
    $actual = (Get-FileHash -Algorithm SHA256 $archivePath).Hash.ToLowerInvariant()
    if ($expected -ne $actual) {
        throw "Checksum verification failed for $archiveName."
    }

    return $archivePath
}

function Extract-Binary {
    param(
        [string]$ArchivePath,
        [string]$Service,
        [string]$TempDir
    )

    $extractDir = Join-Path $TempDir "extract-$Service"
    if (Test-Path $extractDir) {
        Remove-Item -Recurse -Force $extractDir
    }
    New-Item -ItemType Directory -Force -Path $extractDir | Out-Null
    Expand-Archive -Path $ArchivePath -DestinationPath $extractDir -Force

    $binary = Get-ChildItem -Path $extractDir -Recurse -File -Filter "$Service.exe" | Select-Object -First 1
    $example = Get-ChildItem -Path $extractDir -Recurse -File -Filter "$Service.json.example" | Select-Object -First 1

    if (-not $binary -or -not $example) {
        throw "The archive layout is missing the expected binary or example config."
    }

    New-Item -ItemType Directory -Force -Path (Join-Path $script:InstallDir "bin") | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $script:InstallDir "share\examples") | Out-Null

    Copy-Item $binary.FullName (Join-Path $script:InstallDir "bin\$Service.exe") -Force
    Copy-Item $example.FullName (Join-Path $script:InstallDir "share\examples\$Service.json.example") -Force
}

function Write-LanceDbConfig {
    param([string]$Instance, [string]$Host, [int]$Port)
    $configDir = Join-Path $script:InstallDir "config"
    $dataDir = Join-Path $script:InstallDir "data\lancedb\$Instance"
    New-Item -ItemType Directory -Force -Path $configDir, $dataDir | Out-Null

    @{
        host = $Host
        port = $Port
        db_path = $dataDir
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 (Join-Path $configDir "vldb-lancedb-$Instance.json")
}

function Write-DuckDbConfig {
    param([string]$Instance, [string]$Host, [int]$Port)
    $configDir = Join-Path $script:InstallDir "config"
    $dataDir = Join-Path $script:InstallDir "data\duckdb\$Instance"
    New-Item -ItemType Directory -Force -Path $configDir, $dataDir | Out-Null

    @{
        host = $Host
        port = $Port
        db_path = (Join-Path $dataDir "duckdb.db")
        memory_limit = "2GB"
        threads = 4
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 (Join-Path $configDir "vldb-duckdb-$Instance.json")
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

function Write-GlobalConfig {
    New-Item -ItemType Directory -Force -Path $script:GlobalHome | Out-Null
    @{
        language = "en"
        install_dir = $script:InstallDir
        release_tag = $script:ReleaseTag
        script_version = $script:ControllerScriptVersion
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 $script:GlobalConfig
}

function Install-ManagerScripts {
    $sourceDir = Split-Path -Parent $PSCommandPath
    $binDir = Join-Path $script:InstallDir "bin"
    $managerScript = Join-Path $binDir "vldg.ps1"
    New-Item -ItemType Directory -Force -Path $binDir | Out-Null

    if (Test-Path (Join-Path $sourceDir "vldg.ps1")) {
        Copy-Item (Join-Path $sourceDir "vldg.ps1") $managerScript -Force
        Copy-Item (Join-Path $sourceDir "vldg.cmd") (Join-Path $binDir "vldg.cmd") -Force
    } else {
        Download-FileWithProgress -Url "$RawBaseUrl/vldg.ps1" -OutFile $managerScript -Label "vldg.ps1"
        Download-FileWithProgress -Url "$RawBaseUrl/vldg.cmd" -OutFile (Join-Path $binDir "vldg.cmd") -Label "vldg.cmd"
    }

    $detectedVersion = Get-ScriptVersionFromFile $managerScript
    if ($detectedVersion) {
        $script:ControllerScriptVersion = $detectedVersion
    }
}

function Ensure-PathExports {
    $binDir = Join-Path $script:InstallDir "bin"
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
}

function Get-ServiceName {
    param([string]$Service, [string]$Instance)
    return "VulcanLocalDB-$Service-$Instance"
}

function Get-WinSWTemplatePath {
    return (Join-Path $script:InstallDir "tools\winsw\winsw-template.exe")
}

function Get-ServiceWrapperDir {
    param([string]$Service, [string]$Instance)
    return (Join-Path $script:RunDir "services\$Service-$Instance")
}

function Get-ServiceWrapperExePath {
    param([string]$Service, [string]$Instance)
    $serviceName = Get-ServiceName -Service $Service -Instance $Instance
    return (Join-Path (Get-ServiceWrapperDir -Service $Service -Instance $Instance) "$serviceName.exe")
}

function Get-ServiceWrapperConfigPath {
    param([string]$Service, [string]$Instance)
    $serviceName = Get-ServiceName -Service $Service -Instance $Instance
    return (Join-Path (Get-ServiceWrapperDir -Service $Service -Instance $Instance) "$serviceName.xml")
}

function Escape-XmlText {
    param([string]$Value)
    return [System.Security.SecurityElement]::Escape($Value)
}

function Ensure-ServiceBuilderInstalled {
    $templatePath = Get-WinSWTemplatePath
    if (Test-Path $templatePath) {
        return $templatePath
    }

    if (-not (Confirm-Choice "Windows service builder WinSW is required. Download and install it now?" "Y")) {
        throw "Service registration was cancelled because WinSW is missing."
    }

    if ($env:PROCESSOR_ARCHITECTURE -ne "AMD64") {
        throw "The built-in Windows service builder bootstrap currently supports only x64 Windows."
    }

    $tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("winsw-" + [guid]::NewGuid().ToString("N"))
    $downloadPath = Join-Path $tempDir "WinSW-x64.exe"

    try {
        New-Item -ItemType Directory -Force -Path $tempDir, (Split-Path -Parent $templatePath) | Out-Null
        Download-FileWithProgress -Url "https://github.com/winsw/winsw/releases/download/$WinSWVersion/WinSW-x64.exe" -OutFile $downloadPath -Label "WinSW-x64.exe"
        Copy-Item $downloadPath $templatePath -Force
    } finally {
        Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
    }

    return $templatePath
}

function Remove-LegacyStartupTask {
    param([string]$Service, [string]$Instance)

    Unregister-ScheduledTask -TaskName (Get-ServiceName -Service $Service -Instance $Instance) -Confirm:$false -ErrorAction SilentlyContinue
    Remove-Item (Join-Path $script:RunDir "$Service-$Instance.cmd") -Force -ErrorAction SilentlyContinue
}

function Write-ServiceWrapperConfig {
    param([string]$Service, [string]$Instance)

    $serviceName = Get-ServiceName -Service $Service -Instance $Instance
    $wrapperDir = Get-ServiceWrapperDir -Service $Service -Instance $Instance
    $wrapperExe = Get-ServiceWrapperExePath -Service $Service -Instance $Instance
    $wrapperConfig = Get-ServiceWrapperConfigPath -Service $Service -Instance $Instance
    $binaryPath = Join-Path $script:InstallDir "bin\$Service.exe"
    $jsonConfig = Join-Path $script:InstallDir "config\$Service-$Instance.json"
    $logDir = Join-Path $script:GlobalHome "logs\$Service-$Instance"

    New-Item -ItemType Directory -Force -Path $wrapperDir, $logDir | Out-Null
    Copy-Item (Ensure-ServiceBuilderInstalled) $wrapperExe -Force

    $escapedServiceName = Escape-XmlText $serviceName
    $escapedDisplayName = Escape-XmlText ("VulcanLocalDB $Service $Instance")
    $escapedBinary = Escape-XmlText $binaryPath
    $escapedConfig = Escape-XmlText $jsonConfig
    $escapedWorkDir = Escape-XmlText $script:InstallDir
    $escapedLogDir = Escape-XmlText $logDir

    @"
<service>
  <id>$escapedServiceName</id>
  <name>$escapedDisplayName</name>
  <description>$escapedDisplayName</description>
  <executable>$escapedBinary</executable>
  <arguments>--config &quot;$escapedConfig&quot;</arguments>
  <workingdirectory>$escapedWorkDir</workingdirectory>
  <startmode>Automatic</startmode>
  <stoptimeout>15 sec</stoptimeout>
  <onfailure action="restart" delay="10 sec" />
  <onfailure action="restart" delay="10 sec" />
  <onfailure action="restart" delay="30 sec" />
  <logpath>$escapedLogDir</logpath>
  <log mode="roll" />
</service>
"@ | Set-Content -Encoding UTF8 $wrapperConfig

    return $wrapperExe
}

function Register-WindowsService {
    param([string]$Service, [string]$Instance)

    $wrapperExe = Write-ServiceWrapperConfig -Service $Service -Instance $Instance
    Remove-LegacyStartupTask -Service $Service -Instance $Instance

    & $wrapperExe stop 2>$null
    & $wrapperExe uninstall 2>$null
    & $wrapperExe install
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to install the Windows service."
    }

    & $wrapperExe start
    if ($LASTEXITCODE -ne 0) {
        throw "The Windows service was installed, but it failed to start."
    }
}

function Unregister-WindowsService {
    param([string]$Service, [string]$Instance)

    $wrapperExe = Get-ServiceWrapperExePath -Service $Service -Instance $Instance
    Remove-LegacyStartupTask -Service $Service -Instance $Instance

    if (Test-Path $wrapperExe) {
        & $wrapperExe stop 2>$null
        & $wrapperExe uninstall 2>$null
    }

    Remove-Item (Get-ServiceWrapperDir -Service $Service -Instance $Instance) -Recurse -Force -ErrorAction SilentlyContinue
}

function Main {
    $tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("vulcanlocaldb-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Force -Path $tempDir | Out-Null

    try {
        Show-Banner
        Show-UpdateNotice
        Choose-InstallMode
        Choose-InstallDir

        if ($script:InstallMode -eq "full") {
            Choose-NetworkSettings

            $release = Get-LatestRelease
            $target = Get-TargetTriple

            Write-Info "Resolved release tag: $ReleaseTag"

            $lancedbArchive = Download-AssetPair -Service "vldb-lancedb" -Tag $ReleaseTag -Target $target -TempDir $tempDir -Release $release
            $duckdbArchive = Download-AssetPair -Service "vldb-duckdb" -Tag $ReleaseTag -Target $target -TempDir $tempDir -Release $release

            Extract-Binary -ArchivePath $lancedbArchive -Service "vldb-lancedb" -TempDir $tempDir
            Extract-Binary -ArchivePath $duckdbArchive -Service "vldb-duckdb" -TempDir $tempDir

            Write-LanceDbConfig -Instance "default" -Host $HostBind -Port $LanceDbPort
            Write-DuckDbConfig -Instance "default" -Host $HostBind -Port $DuckDbPort
        }

        Install-ManagerScripts
        Write-GlobalConfig
        Ensure-PathExports

        if ($script:InstallMode -eq "full" -and (Confirm-Choice "Register both services for auto start and auto restart?" "N")) {
            Register-WindowsService -Service "vldb-lancedb" -Instance "default"
            Register-WindowsService -Service "vldb-duckdb" -Instance "default"
        }

        if ($script:InstallMode -eq "full") {
            Write-Info "Installation completed."
        } else {
            Write-Info "Controller installation completed."
        }
        Write-Info "Launcher: $InstallDir\bin\vldg.cmd"
    } finally {
        Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
    }
}

Main
