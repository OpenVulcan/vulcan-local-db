param(
    [switch]$FromInstaller
)

$ErrorActionPreference = "Stop"

$ScriptVersion = "0.1.20"
$RepoSlug = "OpenVulcan/vulcan-local-db"
$RepoUrl = "https://github.com/OpenVulcan/vulcan-local-db"
$RawBaseUrl = "https://raw.githubusercontent.com/$RepoSlug/main/scripts"
$GlobalHome = Join-Path $HOME ".vulcan\vldb"
$GlobalConfig = Join-Path $GlobalHome "config.json"
$RunDir = Join-Path $GlobalHome "run"
$InstallDir = $null
$ReleaseTag = $null
$InstalledScriptVersion = $ScriptVersion
$Initialized = $false
$LanceDbRoot = Join-Path $GlobalHome "lancedb"
$DuckDbRoot = Join-Path $GlobalHome "duckdb"
$WinSWVersion = "v2.12.0"
$LatestRelease = $null

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

function Write-Warn {
    param([string]$Message)
    Write-ColorLine -Message $Message -Color Red
}

function Write-Success {
    param([string]$Message)
    Write-ColorLine -Message $Message -Color Green
}

function Write-Running {
    param([string]$Message)
    Write-ColorLine -Message ("Running " + $Message + "...") -Color Yellow
}

function Write-Done {
    param([string]$Message)
    Write-ColorLine -Message ($Message + " completed.") -Color Green
}

function Write-Panel {
    param(
        [string]$Title,
        [ConsoleColor]$BorderColor = [ConsoleColor]::Green,
        [ConsoleColor]$TitleColor = [ConsoleColor]::Green
    )

    Write-Host ""
    Write-ColorLine -Message "====================================" -Color $BorderColor
    Write-ColorLine -Message $Title -Color $TitleColor
    Write-ColorLine -Message "====================================" -Color $BorderColor
}

function Write-MenuSeparator {
    Write-ColorLine -Message "------------------------------------" -Color Green
}

function Invoke-MenuAction {
    param(
        [string]$Label,
        [scriptblock]$Action
    )

    Write-Panel -Title $Label
    Write-Running $Label
    try {
        & $Action
        Write-Done $Label
    } catch {
        Write-Warn ($Label + " failed: " + $_.Exception.Message)
    }
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

function Read-Default {
    param([string]$PromptText, [string]$DefaultValue)

    $prompt = "$PromptText [$DefaultValue]"
    $value = Read-Host $prompt
    if ([string]::IsNullOrWhiteSpace($value)) { return $DefaultValue }
    return $value
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

function Resolve-InstallDir {
    $config = Read-Config

    if ($config -and $config.install_dir) {
        $script:InstallDir = [string]$config.install_dir
    } else {
        $script:InstallDir = Split-Path -Parent (Split-Path -Parent $PSCommandPath)
    }

    if ($config -and $config.release_tag) {
        $script:ReleaseTag = [string]$config.release_tag
    }
    if ($config -and $config.script_version) {
        $script:InstalledScriptVersion = [string]$config.script_version
    }
    if ($config -and $config.lancedb_root) {
        $script:LanceDbRoot = [string]$config.lancedb_root
    }
    if ($config -and $config.duckdb_root) {
        $script:DuckDbRoot = [string]$config.duckdb_root
    }
    if ($config -and $null -ne $config.initialized) {
        $script:Initialized = [bool]$config.initialized
    }
}

function Write-Config {
    New-Item -ItemType Directory -Force -Path $script:GlobalHome | Out-Null
    @{
        language = "en"
        install_dir = $script:InstallDir
        release_tag = $script:ReleaseTag
        script_version = $script:InstalledScriptVersion
        lancedb_root = $script:LanceDbRoot
        duckdb_root = $script:DuckDbRoot
        initialized = $script:Initialized
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

function Clear-UserEnvironmentValue {
    param([string]$Name, [string]$ExpectedValue)

    $currentValue = [Environment]::GetEnvironmentVariable($Name, "User")
    if ($currentValue -and [string]::Equals($currentValue, $ExpectedValue, [System.StringComparison]::OrdinalIgnoreCase)) {
        [Environment]::SetEnvironmentVariable($Name, $null, "User")
    }
}

function Start-DeferredCleanup {
    param([string[]]$Paths)

    $targets = @($Paths | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if (-not $targets -or $targets.Count -eq 0) {
        return
    }

    $quotedTargets = $targets | ForEach-Object { '"' + $_.Replace('"', '""') + '"' }
    $cleanupCommand = "ping 127.0.0.1 -n 4 >nul"
    foreach ($targetPath in $quotedTargets) {
        $cleanupCommand += " & rmdir /s /q $targetPath 2>nul & del /f /q $targetPath 2>nul"
    }

    Start-Process -FilePath "cmd.exe" -ArgumentList "/c", $cleanupCommand -WindowStyle Hidden | Out-Null
}

function Ensure-CmdLauncher {
    $binDir = Join-Path $script:InstallDir "bin"
    $cmdPath = Join-Path $binDir "vldb.cmd"

    if (-not (Test-Path $binDir)) {
        return
    }

    @"
@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0vldb.ps1" %*
"@ | Set-Content -Encoding ASCII $cmdPath
}

function Get-DefaultDataRoot {
    param([string]$Service)

    if ($Service -eq "vldb-lancedb") {
        return (Join-Path $script:GlobalHome "lancedb")
    }

    return (Join-Path $script:GlobalHome "duckdb")
}

function Get-DefaultPort {
    param([string]$Service)

    if ($Service -eq "vldb-lancedb") {
        return 19301
    }

    return 19401
}

function Get-LegacyServiceName {
    param([string]$Service, [string]$Instance)
    return "VulcanLocalDB-$Service-$Instance"
}

function Get-DefaultInstanceDataPath {
    param(
        [string]$Service,
        [string]$Instance,
        [string]$LanceRoot = $script:LanceDbRoot,
        [string]$DuckRoot = $script:DuckDbRoot
    )

    if ($Service -eq "vldb-lancedb") {
        return (Join-Path $LanceRoot $Instance)
    }

    return (Join-Path (Join-Path $DuckRoot $Instance) "duckdb.db")
}

function Resolve-NormalizedPath {
    param([string]$PathValue)

    $fullPath = [System.IO.Path]::GetFullPath($PathValue)
    if ($fullPath.Length -gt 3) {
        $fullPath = $fullPath.TrimEnd('\', '/')
    }

    return $fullPath
}

function Test-PathsOverlap {
    param([string]$LeftPath, [string]$RightPath)

    $leftNormalized = Resolve-NormalizedPath $LeftPath
    $rightNormalized = Resolve-NormalizedPath $RightPath

    if ([string]::Equals($leftNormalized, $rightNormalized, [System.StringComparison]::OrdinalIgnoreCase)) {
        return $true
    }

    $separator = [System.IO.Path]::DirectorySeparatorChar
    $leftPrefix = $leftNormalized + $separator
    $rightPrefix = $rightNormalized + $separator

    return $rightNormalized.StartsWith($leftPrefix, [System.StringComparison]::OrdinalIgnoreCase) -or
        $leftNormalized.StartsWith($rightPrefix, [System.StringComparison]::OrdinalIgnoreCase)
}

function Test-ValidPort {
    param([string]$Value)

    if ($Value -notmatch '^\d+$') { return $false }
    $port = [int]$Value
    return $port -ge 1 -and $port -le 65535
}

function Test-ValidBindIp {
    param([string]$Value)

    if ([string]::IsNullOrWhiteSpace($Value)) {
        return $false
    }

    if ($Value -notmatch '^\d{1,3}(\.\d{1,3}){3}$') {
        return $false
    }

    foreach ($segment in ($Value -split '\.')) {
        if ([int]$segment -lt 0 -or [int]$segment -gt 255) {
            return $false
        }
    }

    return $true
}

function Test-ValidInstanceName {
    param([string]$Value)

    return $Value -match '^[A-Za-z0-9][A-Za-z0-9_-]{0,31}$'
}

function Test-ValidServiceName {
    param([string]$Value)

    return $Value -match '^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$'
}

function Get-InstanceFiles {
    $configDir = Join-Path $script:InstallDir "config"
    if (-not (Test-Path $configDir)) { return @() }

    return Get-ChildItem -Path $configDir -File | Where-Object {
        $_.Name -like "vldb-lancedb-*.json" -or $_.Name -like "vldb-duckdb-*.json"
    } | Sort-Object Name
}

function Get-InstanceMeta {
    param([System.IO.FileInfo]$File)

    $parts = $File.BaseName -split "-"
    $service = ($parts[0..1] -join "-")
    $instance = ($parts[2..($parts.Length - 1)] -join "-")
    return @{ service = $service; instance = $instance }
}

function Get-InstanceConfigPath {
    param([string]$Service, [string]$Instance)
    return (Join-Path $script:InstallDir "config\$Service-$Instance.json")
}

function Read-InstanceConfig {
    param([string]$Path)

    if (-not (Test-Path $Path)) { return $null }

    try {
        return Get-Content $Path -Raw | ConvertFrom-Json
    } catch {
        return $null
    }
}

function Get-ServiceRegistrationName {
    param(
        [string]$Service,
        [string]$Instance,
        [string]$ConfigPath = $null
    )

    $path = $ConfigPath
    if (-not $path) {
        $path = Get-InstanceConfigPath -Service $Service -Instance $Instance
    }

    $config = Read-InstanceConfig $path
    if ($config -and $config.service_name) {
        return [string]$config.service_name
    }

    return (Get-LegacyServiceName -Service $Service -Instance $Instance)
}

function Get-ConfigDbPath {
    param([string]$Path)

    $config = Read-InstanceConfig $Path
    if ($config -and $config.db_path) {
        return [string]$config.db_path
    }

    return $null
}

function Get-ConfigPort {
    param([string]$Path)

    $config = Read-InstanceConfig $Path
    if ($config -and $config.port) {
        return [int]$config.port
    }

    return $null
}

function Get-ServiceNameConflictMessage {
    param(
        [string]$CandidateName,
        [string]$Service,
        [string]$Instance
    )

    foreach ($file in Get-InstanceFiles) {
        $meta = Get-InstanceMeta $file
        if ($meta.service -eq $Service -and $meta.instance -eq $Instance) {
            continue
        }

        $existingName = Get-ServiceRegistrationName -Service $meta.service -Instance $meta.instance -ConfigPath $file.FullName
        if ([string]::Equals($existingName, $CandidateName, [System.StringComparison]::OrdinalIgnoreCase)) {
            return "Service name conflicts with $($meta.service)/$($meta.instance): $existingName"
        }
    }

    return $null
}

function Get-ServiceNameValidationError {
    param(
        [string]$CandidateName,
        [string]$Service,
        [string]$Instance,
        [string]$CurrentName = $null
    )

    if (-not (Test-ValidServiceName $CandidateName)) {
        return "Service names may contain letters, numbers, dot, dash, and underscore."
    }

    $conflict = Get-ServiceNameConflictMessage -CandidateName $CandidateName -Service $Service -Instance $Instance
    if ($conflict) {
        return $conflict
    }

    $existingService = Get-Service -Name $CandidateName -ErrorAction SilentlyContinue
    if ($existingService -and -not [string]::Equals($CandidateName, $CurrentName, [System.StringComparison]::OrdinalIgnoreCase)) {
        return "A Windows service named '$CandidateName' already exists."
    }

    return $null
}

function New-UniqueServiceName {
    param(
        [string]$Service,
        [string]$Instance,
        [string]$PreferredName = $null,
        [string]$CurrentName = $null
    )

    $baseName = if ([string]::IsNullOrWhiteSpace($PreferredName)) {
        Get-LegacyServiceName -Service $Service -Instance $Instance
    } else {
        $PreferredName
    }

    $candidate = $baseName
    $suffix = 2

    while ($true) {
        $validationError = Get-ServiceNameValidationError -CandidateName $candidate -Service $Service -Instance $Instance -CurrentName $CurrentName
        if (-not $validationError) {
            return $candidate
        }

        if (-not [string]::IsNullOrWhiteSpace($CurrentName) -and [string]::Equals($candidate, $CurrentName, [System.StringComparison]::OrdinalIgnoreCase)) {
            return $candidate
        }

        $candidate = "$baseName-$suffix"
        $suffix += 1
    }
}

function Get-ConflictMessageForDataPath {
    param(
        [string]$CandidatePath,
        [string]$Service,
        [string]$Instance
    )

    foreach ($file in Get-InstanceFiles) {
        $meta = Get-InstanceMeta $file
        if ($meta.service -eq $Service -and $meta.instance -eq $Instance) {
            continue
        }

        $existingPath = Get-ConfigDbPath $file.FullName
        if ([string]::IsNullOrWhiteSpace($existingPath)) {
            continue
        }

        if (Test-PathsOverlap $CandidatePath $existingPath) {
            return "Data path conflicts with $($meta.service)/$($meta.instance): $existingPath"
        }
    }

    return $null
}

function Get-DataPathValidationError {
    param(
        [string]$CandidatePath,
        [string]$Service,
        [string]$Instance
    )

    if ([string]::IsNullOrWhiteSpace($CandidatePath) -or -not [System.IO.Path]::IsPathRooted($CandidatePath)) {
        return "Please use a legal absolute data path."
    }

    try {
        [System.IO.Path]::GetFullPath($CandidatePath) | Out-Null
    } catch {
        return "Please use a legal absolute data path."
    }

    if (Test-PathsOverlap $script:InstallDir $CandidatePath) {
        return "Database paths must stay outside the installation directory."
    }

    $conflict = Get-ConflictMessageForDataPath -CandidatePath $CandidatePath -Service $Service -Instance $Instance
    if ($conflict) {
        return $conflict
    }

    return $null
}

function Test-PortAvailable {
    param([int]$Port)

    $listener = $null
    try {
        $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Any, $Port)
        $listener.Start()
        return $true
    } catch {
        return $false
    } finally {
        if ($listener) {
            $listener.Stop()
        }
    }
}

function Test-RegisteredByName {
    param([string]$ServiceName)
    return $null -ne (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue)
}

function Test-ServiceRunningByName {
    param([string]$ServiceName)

    $service = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
    return $service -and $service.Status -eq [System.ServiceProcess.ServiceControllerStatus]::Running
}

function Wait-ForServiceStatus {
    param(
        [string]$ServiceName,
        [System.ServiceProcess.ServiceControllerStatus]$Status,
        [int]$TimeoutSeconds = 20
    )

    $service = Get-Service -Name $ServiceName -ErrorAction Stop
    $service.WaitForStatus($Status, [TimeSpan]::FromSeconds($TimeoutSeconds))
}

function Get-PortConflictMessage {
    param(
        [int]$CandidatePort,
        [string]$Service,
        [string]$Instance
    )

    foreach ($file in Get-InstanceFiles) {
        $meta = Get-InstanceMeta $file
        if ($meta.service -eq $Service -and $meta.instance -eq $Instance) {
            continue
        }

        $existingPort = Get-ConfigPort $file.FullName
        if ($null -ne $existingPort -and [int]$existingPort -eq $CandidatePort) {
            return "Port $CandidatePort is already reserved by $($meta.service)/$($meta.instance). Please choose another port."
        }
    }

    return $null
}

function Get-PortValidationError {
    param(
        [string]$CandidatePort,
        [string]$Service,
        [string]$Instance,
        [int]$CurrentPort = 0,
        [string]$CurrentServiceName = $null
    )

    if (-not (Test-ValidPort $CandidatePort)) {
        return "Invalid port. Please enter an integer between 1 and 65535."
    }

    $port = [int]$CandidatePort
    $conflict = Get-PortConflictMessage -CandidatePort $port -Service $Service -Instance $Instance
    if ($conflict) {
        return $conflict
    }

    if ($CurrentPort -eq $port -and -not [string]::IsNullOrWhiteSpace($CurrentServiceName)) {
        if (Test-ServiceRunningByName $CurrentServiceName) {
            return $null
        }

        if (Test-PortAvailable $port) {
            return $null
        }

        return "Port $port is already in use by another service, container, or process. Please choose another port."
    }

    if (Test-PortAvailable $port) {
        return $null
    }

    return "Port $port is already in use by another service, container, or process. Please choose another port."
}

function Get-BindIpValidationError {
    param([string]$CandidateIp)

    if (Test-ValidBindIp $CandidateIp) {
        return $null
    }

    return "Invalid bind IP. Please enter a valid IPv4 address."
}

function Is-Initialized {
    if ($script:Initialized) {
        return $true
    }

    return (Get-InstanceFiles).Count -gt 0
}

function Choose-Service {
    Write-Panel -Title "Service Selection"
    while ($true) {
        Write-Host "0. Back"
        Write-MenuSeparator
        Write-Host "1. LanceDB"
        Write-Host "2. DuckDB"
        $choice = Read-Host "Choose service [1/2/0]"
        switch ($choice) {
            "1" { return "vldb-lancedb" }
            "2" { return "vldb-duckdb" }
            "0" { return $null }
            default { Write-Info "Please input 1, 2, or 0." }
        }
    }
}

function Choose-InstanceFile {
    $files = Get-InstanceFiles
    if (-not $files -or $files.Count -eq 0) {
        Write-Info "No installed instances were found."
        return $null
    }

    Write-Panel -Title "Installed Instances"
    Write-Host "0. Back"
    Write-MenuSeparator
    for ($index = 0; $index -lt $files.Count; $index++) {
        Write-Host ("{0}. {1}" -f ($index + 1), $files[$index].BaseName)
    }

    while ($true) {
        $choice = Read-Host "Select instance"
        if ($choice -eq "0") {
            return $null
        }
        if ($choice -match '^\d+$') {
            $selected = [int]$choice - 1
            if ($selected -ge 0 -and $selected -lt $files.Count) {
                return $files[$selected]
            }
        }
        Write-Info "Invalid selection. Please choose a listed number or 0."
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
    param([string]$ScriptName)

    $content = Invoke-TextDownload "$RawBaseUrl/$ScriptName"
    if (-not $content) { return $null }

    $match = [regex]::Match($content, '\$ScriptVersion\s*=\s*"([^"]+)"')
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

function Get-LatestRelease {
    if ($script:LatestRelease) {
        return $script:LatestRelease
    }

    $script:LatestRelease = Invoke-RestMethod -Uri "https://api.github.com/repos/$RepoSlug/releases/latest"
    if (-not $script:LatestRelease.tag_name) {
        throw "Unable to resolve the latest release tag."
    }

    return $script:LatestRelease
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

function Download-ServiceArchive {
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

function Install-ServiceBinaryFromArchive {
    param(
        [string]$ArchivePath,
        [string]$Service,
        [string]$TempDir
    )

    $extractDir = Join-Path $TempDir "extract-$Service"
    if (Test-Path $extractDir) {
        Remove-Item -Recurse -Force $extractDir -ErrorAction SilentlyContinue
    }
    New-Item -ItemType Directory -Force -Path $extractDir | Out-Null

    Expand-Archive -Path $ArchivePath -DestinationPath $extractDir -Force
    $binary = Get-ChildItem -Path $extractDir -Recurse -File -Filter "$Service.exe" | Select-Object -First 1
    $example = Get-ChildItem -Path $extractDir -Recurse -File -Filter "$Service.json.example" | Select-Object -First 1

    if (-not $binary -or -not $example) {
        throw "The archive layout is missing the expected binary or example config."
    }

    New-Item -ItemType Directory -Force -Path (Join-Path $script:InstallDir "bin"), (Join-Path $script:InstallDir "share\examples") | Out-Null
    Copy-Item $binary.FullName (Join-Path $script:InstallDir "bin\$Service.exe") -Force
    Copy-Item $example.FullName (Join-Path $script:InstallDir "share\examples\$Service.json.example") -Force
}

function Install-ServiceBinary {
    param(
        [string]$Service,
        [string]$Tag
    )

    $release = Get-LatestRelease
    $target = Get-TargetTriple
    $resolvedTag = if ($Tag) { $Tag } else { [string]$release.tag_name }
    $tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("vulcanlocaldb-" + [guid]::NewGuid().ToString("N"))

    New-Item -ItemType Directory -Force -Path $tempDir | Out-Null
    try {
        $archivePath = Download-ServiceArchive -Service $Service -Tag $resolvedTag -Target $target -TempDir $tempDir -Release $release
        Install-ServiceBinaryFromArchive -ArchivePath $archivePath -Service $Service -TempDir $tempDir
    } finally {
        Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
    }
}

function Ensure-ServiceBinaryInstalled {
    param([string]$Service)

    $binaryPath = Join-Path $script:InstallDir "bin\$Service.exe"
    if (Test-Path $binaryPath) {
        return
    }

    $release = Get-LatestRelease
    $script:ReleaseTag = [string]$release.tag_name
    Install-ServiceBinary -Service $Service -Tag $script:ReleaseTag
    Write-Config
}

function Get-InstalledServiceKinds {
    $services = New-Object System.Collections.Generic.HashSet[string]([System.StringComparer]::OrdinalIgnoreCase)

    foreach ($file in Get-InstanceFiles) {
        $meta = Get-InstanceMeta $file
        [void]$services.Add($meta.service)
    }

    foreach ($service in @("vldb-lancedb", "vldb-duckdb")) {
        if (Test-Path (Join-Path $script:InstallDir "bin\$service.exe")) {
            [void]$services.Add($service)
        }
    }

    return @($services)
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
    return (Join-Path (Get-ServiceWrapperDir -Service $Service -Instance $Instance) "$Service-$Instance.exe")
}

function Get-ServiceWrapperConfigPath {
    param([string]$Service, [string]$Instance)
    return (Join-Path (Get-ServiceWrapperDir -Service $Service -Instance $Instance) "$Service-$Instance.xml")
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

    if ($env:PROCESSOR_ARCHITECTURE -ne "AMD64") {
        throw "The built-in Windows service builder bootstrap currently supports only x64 Windows."
    }

    $tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("winsw-" + [guid]::NewGuid().ToString("N"))
    $downloadPath = Join-Path $tempDir "WinSW-x64.exe"

    try {
        Write-Step "Downloading WinSW service wrapper to tools directory"
        New-Item -ItemType Directory -Force -Path $tempDir, (Split-Path -Parent $templatePath) | Out-Null
        Download-FileWithProgress -Url "https://github.com/winsw/winsw/releases/download/$WinSWVersion/WinSW-x64.exe" -OutFile $downloadPath -Label "WinSW-x64.exe"
        Copy-Item $downloadPath $templatePath -Force
    } finally {
        Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
    }

    return $templatePath
}

function Remove-LegacyStartupTask {
    param([string]$RegisteredName)

    if ([string]::IsNullOrWhiteSpace($RegisteredName)) {
        return
    }

    Unregister-ScheduledTask -TaskName $RegisteredName -Confirm:$false -ErrorAction SilentlyContinue
}

function Write-ServiceWrapperConfig {
    param(
        [string]$Service,
        [string]$Instance,
        [string]$RegisteredName
    )

    $wrapperDir = Get-ServiceWrapperDir -Service $Service -Instance $Instance
    $wrapperExe = Get-ServiceWrapperExePath -Service $Service -Instance $Instance
    $wrapperConfig = Get-ServiceWrapperConfigPath -Service $Service -Instance $Instance
    $binaryPath = Join-Path $script:InstallDir "bin\$Service.exe"
    $jsonConfig = Get-InstanceConfigPath -Service $Service -Instance $Instance
    $logDir = Join-Path $script:GlobalHome "logs\$Service-$Instance"

    New-Item -ItemType Directory -Force -Path $wrapperDir, $logDir | Out-Null
    Copy-Item (Ensure-ServiceBuilderInstalled) $wrapperExe -Force

    $escapedName = Escape-XmlText $RegisteredName
    $escapedBinary = Escape-XmlText $binaryPath
    $escapedConfig = Escape-XmlText $jsonConfig
    $escapedWorkDir = Escape-XmlText $script:InstallDir
    $escapedLogDir = Escape-XmlText $logDir

    @"
<service>
  <id>$escapedName</id>
  <name>$escapedName</name>
  <description>$escapedName</description>
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

function Remove-WindowsServiceByName {
    param([string]$RegisteredName)

    if ([string]::IsNullOrWhiteSpace($RegisteredName)) {
        return
    }

    $service = Get-Service -Name $RegisteredName -ErrorAction SilentlyContinue
    if (-not $service) {
        return
    }

    if ($service.Status -ne [System.ServiceProcess.ServiceControllerStatus]::Stopped) {
        Stop-Service -Name $RegisteredName -Force -ErrorAction SilentlyContinue
        try {
            $service.WaitForStatus([System.ServiceProcess.ServiceControllerStatus]::Stopped, [TimeSpan]::FromSeconds(20))
        } catch {
        }
    }

    & sc.exe delete $RegisteredName | Out-Null

    for ($index = 0; $index -lt 20; $index++) {
        Start-Sleep -Milliseconds 250
        if (-not (Get-Service -Name $RegisteredName -ErrorAction SilentlyContinue)) {
            break
        }
    }
}

function Test-Registered {
    param(
        [string]$Service,
        [string]$Instance,
        [string]$ConfigPath = $null
    )

    $registeredName = Get-ServiceRegistrationName -Service $Service -Instance $Instance -ConfigPath $ConfigPath
    return (Test-RegisteredByName $registeredName)
}

function Register-Instance {
    param([string]$Service, [string]$Instance)

    $registeredName = Get-ServiceRegistrationName -Service $Service -Instance $Instance

    Remove-LegacyStartupTask -RegisteredName $registeredName
    Remove-WindowsServiceByName -RegisteredName $registeredName

    $wrapperExe = Write-ServiceWrapperConfig -Service $Service -Instance $Instance -RegisteredName $registeredName

    & $wrapperExe install
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to install the Windows service."
    }

    & $wrapperExe start
    if ($LASTEXITCODE -ne 0) {
        throw "The Windows service was installed, but it failed to start."
    }
}

function Unregister-Instance {
    param(
        [string]$Service,
        [string]$Instance,
        [string]$RegisteredName = $null
    )

    $nameToRemove = if ($RegisteredName) { $RegisteredName } else { Get-ServiceRegistrationName -Service $Service -Instance $Instance }

    Remove-LegacyStartupTask -RegisteredName $nameToRemove
    Remove-WindowsServiceByName -RegisteredName $nameToRemove
    Remove-Item (Get-ServiceWrapperDir -Service $Service -Instance $Instance) -Recurse -Force -ErrorAction SilentlyContinue
}

function Start-InstanceService {
    param([string]$Service, [string]$Instance)

    $registeredName = Get-ServiceRegistrationName -Service $Service -Instance $Instance
    if (-not (Test-RegisteredByName $registeredName)) {
        Write-Info "This instance is not registered. Re-registering it now."
        Register-Instance -Service $Service -Instance $Instance
        return
    }

    Start-Service -Name $registeredName -ErrorAction SilentlyContinue
    Wait-ForServiceStatus -ServiceName $registeredName -Status ([System.ServiceProcess.ServiceControllerStatus]::Running)
}

function Stop-InstanceService {
    param([string]$Service, [string]$Instance)

    $registeredName = Get-ServiceRegistrationName -Service $Service -Instance $Instance
    if (-not (Test-RegisteredByName $registeredName)) {
        Write-Info "This instance is not registered."
        return
    }

    Stop-Service -Name $registeredName -Force -ErrorAction SilentlyContinue
    Wait-ForServiceStatus -ServiceName $registeredName -Status ([System.ServiceProcess.ServiceControllerStatus]::Stopped)
}

function Restart-RegisteredServiceByNameIfRunning {
    param([string]$ServiceName)

    if (-not (Test-RegisteredByName $ServiceName)) {
        return $false
    }

    if (-not (Test-ServiceRunningByName $ServiceName)) {
        return $false
    }

    Stop-Service -Name $ServiceName -Force -ErrorAction SilentlyContinue
    Wait-ForServiceStatus -ServiceName $ServiceName -Status ([System.ServiceProcess.ServiceControllerStatus]::Stopped)
    Start-Service -Name $ServiceName -ErrorAction SilentlyContinue
    Wait-ForServiceStatus -ServiceName $ServiceName -Status ([System.ServiceProcess.ServiceControllerStatus]::Running)
    return $true
}

function Write-ServiceConfig {
    param(
        [string]$Service,
        [string]$Instance,
        [string]$BindHost,
        [int]$Port,
        [string]$DataPath,
        [string]$ServiceName
    )

    if (-not (Test-ValidBindIp $BindHost)) {
        throw "Invalid bind IP. Please enter a valid IPv4 address."
    }

    if (-not (Test-ValidPort ([string]$Port))) {
        throw "Invalid port. Please enter an integer between 1 and 65535."
    }

    $configDir = Join-Path $script:InstallDir "config"
    New-Item -ItemType Directory -Force -Path $configDir | Out-Null

    if ($Service -eq "vldb-lancedb") {
        New-Item -ItemType Directory -Force -Path $DataPath | Out-Null
        @{
            host = $BindHost
            port = $Port
            db_path = $DataPath
            service_name = $ServiceName
        } | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 (Get-InstanceConfigPath $Service $Instance)
    } else {
        $dataDir = Split-Path -Parent $DataPath
        New-Item -ItemType Directory -Force -Path $dataDir | Out-Null
        @{
            host = $BindHost
            port = $Port
            db_path = $DataPath
            memory_limit = "2GB"
            threads = 4
            service_name = $ServiceName
        } | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 (Get-InstanceConfigPath $Service $Instance)
    }
}

function Show-Instances {
    $files = Get-InstanceFiles
    if (-not $files -or $files.Count -eq 0) {
        Write-Info "No installed instances were found."
        return
    }

    Write-Panel -Title "Installed Instances"
    foreach ($file in $files) {
        $meta = Get-InstanceMeta $file
        $config = Read-InstanceConfig $file.FullName
        $serviceName = Get-ServiceRegistrationName -Service $meta.service -Instance $meta.instance -ConfigPath $file.FullName
        $registration = if (Test-Registered -Service $meta.service -Instance $meta.instance -ConfigPath $file.FullName) { "registered" } else { "not registered" }
        $runtime = if (Test-ServiceRunningByName $serviceName) { "running" } else { "stopped" }
        Write-Host ("{0} {1} | {2} | {3}:{4} | {5}/{6} | {7}" -f $meta.service, $meta.instance, $serviceName, $config.host, $config.port, $registration, $runtime, $config.db_path)
    }
}

function Choose-DataRoots {
    $defaultLanceRoot = Get-DefaultDataRoot "vldb-lancedb"
    $defaultDuckRoot = Get-DefaultDataRoot "vldb-duckdb"

    while ($true) {
        $lanceRoot = Read-Default "LanceDB data root" $defaultLanceRoot
        if (-not [System.IO.Path]::IsPathRooted($lanceRoot)) {
            Write-Info "Invalid LanceDB data root."
            continue
        }
        if (Test-PathsOverlap $script:InstallDir $lanceRoot) {
            Write-Info "LanceDB data root must stay outside the installation directory."
            continue
        }
        if ((Test-Path $lanceRoot) -and -not (Test-Path $lanceRoot -PathType Container)) {
            Write-Info "LanceDB data root already exists and is not a directory."
            continue
        }

        $duckRoot = Read-Default "DuckDB data root" $defaultDuckRoot
        if (-not [System.IO.Path]::IsPathRooted($duckRoot)) {
            Write-Info "Invalid DuckDB data root."
            continue
        }
        if (Test-PathsOverlap $script:InstallDir $duckRoot) {
            Write-Info "DuckDB data root must stay outside the installation directory."
            continue
        }
        if ((Test-Path $duckRoot) -and -not (Test-Path $duckRoot -PathType Container)) {
            Write-Info "DuckDB data root already exists and is not a directory."
            continue
        }
        if (Test-PathsOverlap $lanceRoot $duckRoot) {
            Write-Info "LanceDB and DuckDB data roots must not overlap."
            continue
        }

        $defaultLancePath = Get-DefaultInstanceDataPath -Service "vldb-lancedb" -Instance "default" -LanceRoot $lanceRoot -DuckRoot $duckRoot
        $defaultDuckPath = Get-DefaultInstanceDataPath -Service "vldb-duckdb" -Instance "default" -LanceRoot $lanceRoot -DuckRoot $duckRoot

        $lanceError = Get-DataPathValidationError -CandidatePath $defaultLancePath -Service "vldb-lancedb" -Instance "default"
        if ($lanceError) {
            Write-Info $lanceError
            continue
        }

        $duckError = Get-DataPathValidationError -CandidatePath $defaultDuckPath -Service "vldb-duckdb" -Instance "default"
        if ($duckError) {
            Write-Info $duckError
            continue
        }

        $script:LanceDbRoot = Resolve-NormalizedPath $lanceRoot
        $script:DuckDbRoot = Resolve-NormalizedPath $duckRoot
        New-Item -ItemType Directory -Force -Path $script:LanceDbRoot, $script:DuckDbRoot | Out-Null
        return
    }
}

function Prompt-ForPort {
    param(
        [string]$PromptText,
        [int]$DefaultPort,
        [string]$Service,
        [string]$Instance,
        [int]$CurrentPort = 0,
        [string]$CurrentServiceName = $null
    )

    while ($true) {
        $portInput = Read-Default $PromptText "$DefaultPort"
        $validationError = Get-PortValidationError -CandidatePort $portInput -Service $Service -Instance $Instance -CurrentPort $CurrentPort -CurrentServiceName $CurrentServiceName
        if (-not $validationError) {
            return [int]$portInput
        }

        Write-Info $validationError
    }
}

function Prompt-ForBindIp {
    param(
        [string]$PromptText,
        [string]$DefaultValue
    )

    while ($true) {
        $bindIp = Read-Default $PromptText $DefaultValue
        $validationError = Get-BindIpValidationError -CandidateIp $bindIp
        if (-not $validationError) {
            return $bindIp
        }

        Write-Info $validationError
    }
}

function Prompt-ForServiceName {
    param(
        [string]$Service,
        [string]$Instance,
        [string]$DefaultName,
        [string]$CurrentName = $null
    )

    while ($true) {
        $inputName = Read-Default "Windows service name" $DefaultName
        $validationError = Get-ServiceNameValidationError -CandidateName $inputName -Service $Service -Instance $Instance -CurrentName $CurrentName
        if (-not $validationError) {
            return $inputName
        }

        Write-Info $validationError
    }
}

function Initialize-Installation {
    Write-Step "Running initial one-click installation"

    $release = Get-LatestRelease
    $script:ReleaseTag = [string]$release.tag_name

    Choose-DataRoots
    Ensure-ServiceBuilderInstalled | Out-Null

    $bindHost = Prompt-ForBindIp -PromptText "Service bind IP" -DefaultValue "127.0.0.1"

    $lancePort = Prompt-ForPort -PromptText "LanceDB port" -DefaultPort (Get-DefaultPort "vldb-lancedb") -Service "vldb-lancedb" -Instance "default"
    while ($true) {
        $duckPort = Prompt-ForPort -PromptText "DuckDB port" -DefaultPort (Get-DefaultPort "vldb-duckdb") -Service "vldb-duckdb" -Instance "default"
        if ($duckPort -ne $lancePort) {
            break
        }
        Write-Info "LanceDB and DuckDB must use different ports."
    }

    $lanceServiceName = Prompt-ForServiceName -Service "vldb-lancedb" -Instance "default" -DefaultName (New-UniqueServiceName -Service "vldb-lancedb" -Instance "default")
    $duckServiceName = Prompt-ForServiceName -Service "vldb-duckdb" -Instance "default" -DefaultName (New-UniqueServiceName -Service "vldb-duckdb" -Instance "default")
    while ([string]::Equals($lanceServiceName, $duckServiceName, [System.StringComparison]::OrdinalIgnoreCase)) {
        Write-Info "The two default services must not share the same service name."
        $duckServiceName = Prompt-ForServiceName -Service "vldb-duckdb" -Instance "default" -DefaultName (New-UniqueServiceName -Service "vldb-duckdb" -Instance "default" -CurrentName $duckServiceName)
    }

    Install-ServiceBinary -Service "vldb-lancedb" -Tag $script:ReleaseTag
    Install-ServiceBinary -Service "vldb-duckdb" -Tag $script:ReleaseTag

    Write-ServiceConfig -Service "vldb-lancedb" -Instance "default" -BindHost $bindHost -Port $lancePort -DataPath (Get-DefaultInstanceDataPath -Service "vldb-lancedb" -Instance "default") -ServiceName $lanceServiceName
    Write-ServiceConfig -Service "vldb-duckdb" -Instance "default" -BindHost $bindHost -Port $duckPort -DataPath (Get-DefaultInstanceDataPath -Service "vldb-duckdb" -Instance "default") -ServiceName $duckServiceName

    Register-Instance -Service "vldb-lancedb" -Instance "default"
    Register-Instance -Service "vldb-duckdb" -Instance "default"

    $script:Initialized = $true
    $script:InstalledScriptVersion = $ScriptVersion
    Write-Config
    Write-Info "Initial installation completed."
}

function Configure-Instance {
    $file = Choose-InstanceFile
    if (-not $file) { return }

    $meta = Get-InstanceMeta $file
    $config = Read-InstanceConfig $file.FullName
    $currentBindHost = [string]$config.host
    $currentPort = [int]$config.port
    $currentDataPath = Resolve-NormalizedPath ([string]$config.db_path)
    $currentServiceName = Get-ServiceRegistrationName -Service $meta.service -Instance $meta.instance -ConfigPath $file.FullName
    $wasRegistered = Test-Registered -Service $meta.service -Instance $meta.instance -ConfigPath $file.FullName
    $wasRunning = $wasRegistered -and (Test-ServiceRunningByName $currentServiceName)

    while ($true) {
        $bindHost = Prompt-ForBindIp -PromptText "Bind IP" -DefaultValue $currentBindHost

        $port = Prompt-ForPort -PromptText "Port" -DefaultPort $currentPort -Service $meta.service -Instance $meta.instance -CurrentPort $currentPort -CurrentServiceName $currentServiceName

        $dataPathInput = Read-Default "Data path" ([string]$config.db_path)
        $dataPath = Resolve-NormalizedPath $dataPathInput
        if (-not [string]::Equals($currentDataPath, $dataPath, [System.StringComparison]::OrdinalIgnoreCase)) {
            $dataPathError = Get-DataPathValidationError -CandidatePath $dataPathInput -Service $meta.service -Instance $meta.instance
            if ($dataPathError) {
                Write-Info $dataPathError
                continue
            }
        }

        $serviceName = Prompt-ForServiceName -Service $meta.service -Instance $meta.instance -DefaultName (New-UniqueServiceName -Service $meta.service -Instance $meta.instance -PreferredName $currentServiceName -CurrentName $currentServiceName) -CurrentName $currentServiceName
        break
    }

    $bindHostChanged = -not [string]::Equals($currentBindHost, $bindHost, [System.StringComparison]::OrdinalIgnoreCase)
    $portChanged = $currentPort -ne $port
    $dataPathChanged = -not [string]::Equals($currentDataPath, $dataPath, [System.StringComparison]::OrdinalIgnoreCase)
    $serviceNameChanged = -not [string]::Equals($currentServiceName, $serviceName, [System.StringComparison]::OrdinalIgnoreCase)

    if (-not ($bindHostChanged -or $portChanged -or $dataPathChanged -or $serviceNameChanged)) {
        Write-Info "No changes detected for this instance."
        return
    }

    $configBackupPath = Join-Path ([System.IO.Path]::GetTempPath()) ("vulcanlocaldb-config-" + [guid]::NewGuid().ToString("N") + ".json")
    Copy-Item $file.FullName $configBackupPath -Force

    try {
        if (-not $wasRegistered) {
            try {
                Write-ServiceConfig -Service $meta.service -Instance $meta.instance -BindHost $bindHost -Port $port -DataPath $dataPath -ServiceName $serviceName
                Register-Instance -Service $meta.service -Instance $meta.instance
            } catch {
                Copy-Item $configBackupPath $file.FullName -Force
                throw
            }

            Write-Config
            return
        }

        if ($serviceNameChanged) {
            try {
                if ($wasRunning) {
                    Stop-InstanceService -Service $meta.service -Instance $meta.instance
                }

                Write-ServiceConfig -Service $meta.service -Instance $meta.instance -BindHost $bindHost -Port $port -DataPath $dataPath -ServiceName $serviceName
                Unregister-Instance -Service $meta.service -Instance $meta.instance -RegisteredName $currentServiceName
                Register-Instance -Service $meta.service -Instance $meta.instance
            } catch {
                $originalError = $_.Exception.Message
                $rollbackError = $null
                try {
                    Copy-Item $configBackupPath $file.FullName -Force
                    Register-Instance -Service $meta.service -Instance $meta.instance
                    if (-not $wasRunning) {
                        Stop-InstanceService -Service $meta.service -Instance $meta.instance
                    }
                } catch {
                    $rollbackError = $_.Exception.Message
                }

                if ($rollbackError) {
                    throw ("{0} Rollback also failed: {1}" -f $originalError, $rollbackError)
                }

                throw $originalError
            }

            Write-Config
            Write-Info "Configuration updated and the service registration was refreshed."
            return
        }

        if (-not (Test-RegisteredByName $currentServiceName)) {
            try {
                Write-ServiceConfig -Service $meta.service -Instance $meta.instance -BindHost $bindHost -Port $port -DataPath $dataPath -ServiceName $serviceName
                Register-Instance -Service $meta.service -Instance $meta.instance
            } catch {
                Copy-Item $configBackupPath $file.FullName -Force
                throw
            }

            Write-Config
            return
        }

        if ($wasRunning) {
            try {
                Stop-InstanceService -Service $meta.service -Instance $meta.instance
                Write-ServiceConfig -Service $meta.service -Instance $meta.instance -BindHost $bindHost -Port $port -DataPath $dataPath -ServiceName $serviceName
                Start-InstanceService -Service $meta.service -Instance $meta.instance
            } catch {
                $originalError = $_.Exception.Message
                $rollbackError = $null
                try {
                    Copy-Item $configBackupPath $file.FullName -Force
                    Start-InstanceService -Service $meta.service -Instance $meta.instance
                } catch {
                    $rollbackError = $_.Exception.Message
                }

                if ($rollbackError) {
                    throw ("{0} Rollback also failed: {1}" -f $originalError, $rollbackError)
                }

                throw $originalError
            }

            Write-Config
            Write-Info "Configuration updated and the service was restarted."
            return
        }

        Write-ServiceConfig -Service $meta.service -Instance $meta.instance -BindHost $bindHost -Port $port -DataPath $dataPath -ServiceName $serviceName
        Write-Config
        Write-Info "Configuration updated. The new settings will apply the next time the service starts."
    } finally {
        Remove-Item $configBackupPath -Force -ErrorAction SilentlyContinue
    }
}

function Install-SingleInstance {
    $service = Choose-Service
    if (-not $service) { return }
    Ensure-ServiceBuilderInstalled | Out-Null
    Ensure-ServiceBinaryInstalled -Service $service

    while ($true) {
        $instance = Read-Default "Instance name" "default"
        if (-not (Test-ValidInstanceName $instance)) {
            Write-Info "Instance names may contain letters, numbers, dash, and underscore."
            continue
        }
        if (Test-Path (Get-InstanceConfigPath $service $instance)) {
            Write-Info "This instance already exists."
            continue
        }
        break
    }

    while ($true) {
        $bindHost = Prompt-ForBindIp -PromptText "Bind IP" -DefaultValue "127.0.0.1"

        $port = Prompt-ForPort -PromptText "Port" -DefaultPort (Get-DefaultPort $service) -Service $service -Instance $instance

        $defaultDataPath = Get-DefaultInstanceDataPath -Service $service -Instance $instance
        $dataPathInput = Read-Default "Data path" $defaultDataPath
        $dataPathError = Get-DataPathValidationError -CandidatePath $dataPathInput -Service $service -Instance $instance
        if ($dataPathError) {
            Write-Info $dataPathError
            continue
        }

        $serviceName = Prompt-ForServiceName -Service $service -Instance $instance -DefaultName (New-UniqueServiceName -Service $service -Instance $instance)
        $dataPath = Resolve-NormalizedPath $dataPathInput
        break
    }

    Write-ServiceConfig -Service $service -Instance $instance -BindHost $bindHost -Port $port -DataPath $dataPath -ServiceName $serviceName
    $script:Initialized = $true
    Write-Config
    Register-Instance -Service $service -Instance $instance
}

function Start-SingleInstance {
    $file = Choose-InstanceFile
    if (-not $file) { return }

    $meta = Get-InstanceMeta $file
    Start-InstanceService -Service $meta.service -Instance $meta.instance
}

function Stop-SingleInstance {
    $file = Choose-InstanceFile
    if (-not $file) { return }

    $meta = Get-InstanceMeta $file
    Stop-InstanceService -Service $meta.service -Instance $meta.instance
}

function Start-AllInstances {
    foreach ($file in Get-InstanceFiles) {
        $meta = Get-InstanceMeta $file
        Start-InstanceService -Service $meta.service -Instance $meta.instance
    }
}

function Stop-AllInstances {
    foreach ($file in Get-InstanceFiles) {
        $meta = Get-InstanceMeta $file
        Stop-InstanceService -Service $meta.service -Instance $meta.instance
    }
}

function Uninstall-SingleInstance {
    $file = Choose-InstanceFile
    if (-not $file) { return }

    $meta = Get-InstanceMeta $file
    $config = Read-InstanceConfig $file.FullName
    $serviceName = Get-ServiceRegistrationName -Service $meta.service -Instance $meta.instance -ConfigPath $file.FullName

    if (Test-RegisteredByName $serviceName) {
        Unregister-Instance -Service $meta.service -Instance $meta.instance -RegisteredName $serviceName
    }

    Remove-Item $file.FullName -Force
    Write-Info ("Database files were preserved at: {0}" -f $config.db_path)
}

function Update-ManagerScript {
    $managerPath = Join-Path $script:InstallDir "bin\vldb.ps1"
    $tempPath = Join-Path ([System.IO.Path]::GetTempPath()) ("vldb-" + [guid]::NewGuid().ToString("N") + ".ps1")

    try {
        Download-FileWithProgress -Url "$RawBaseUrl/vldb.ps1" -OutFile $tempPath -Label "vldb.ps1"
        Copy-Item $tempPath $managerPath -Force
    } finally {
        Remove-Item $tempPath -Force -ErrorAction SilentlyContinue
    }

    $detectedVersion = [regex]::Match((Get-Content $managerPath -Raw), '\$ScriptVersion\s*=\s*"([^"]+)"').Groups[1].Value
    if ($detectedVersion) {
        $script:InstalledScriptVersion = $detectedVersion
    }

    Ensure-CmdLauncher
    Write-Config
    Write-Info "Manager script updated. Re-run the manager to load the new version."
}

function Update-ApplicationsToTag {
    param([string]$TargetTag)

    $installedKinds = Get-InstalledServiceKinds
    if (-not $installedKinds -or $installedKinds.Count -eq 0) {
        Write-Info "No application binaries are installed yet."
        return
    }

    $runningStates = @{}
    foreach ($file in Get-InstanceFiles) {
        $meta = Get-InstanceMeta $file
        $serviceName = Get-ServiceRegistrationName -Service $meta.service -Instance $meta.instance -ConfigPath $file.FullName
        $runningStates["$($meta.service)|$($meta.instance)"] = [bool](Test-ServiceRunningByName $serviceName)
    }

    $updateError = $null
    $restartErrors = New-Object System.Collections.Generic.List[string]

    try {
        Stop-AllInstances

        foreach ($service in $installedKinds) {
            Install-ServiceBinary -Service $service -Tag $TargetTag
        }
    } catch {
        $updateError = $_.Exception.Message
    } finally {
        foreach ($file in Get-InstanceFiles) {
            $meta = Get-InstanceMeta $file
            $key = "$($meta.service)|$($meta.instance)"
            if ($runningStates.ContainsKey($key) -and $runningStates[$key]) {
                try {
                    Start-InstanceService -Service $meta.service -Instance $meta.instance
                } catch {
                    $restartErrors.Add($_.Exception.Message)
                }
            }
        }
    }

    if ($updateError -or $restartErrors.Count -gt 0) {
        if ($updateError -and $restartErrors.Count -gt 0) {
            throw ("{0} Restart recovery also failed: {1}" -f $updateError, ($restartErrors -join "; "))
        }

        if ($updateError) {
            throw $updateError
        }

        throw ("Restart recovery failed: {0}" -f ($restartErrors -join "; "))
    }

    $script:ReleaseTag = $TargetTag
    Write-Config
    Write-Info "Application binaries updated to $TargetTag."
}

function Check-Updates {
    Write-Info "Checking for updates..."

    $remoteScriptVersion = Get-RemoteScriptVersion -ScriptName "vldb.ps1"
    $latestTag = Try-GetLatestReleaseTag

    Write-Info "Current manager script version: $ScriptVersion"
    if ($remoteScriptVersion) {
        Write-Info "Latest manager script version: $remoteScriptVersion"
        if ((Compare-VersionStrings $remoteScriptVersion $ScriptVersion) -gt 0) {
            Write-Info "Manager script update available."
            if (Confirm-Choice "Update the manager script now?" "Y") {
                Update-ManagerScript
            }
        } else {
            Write-Info "Manager script is up to date."
        }
    } else {
        Write-Info "Latest manager script version: unavailable"
    }

    if ($script:ReleaseTag) {
        Write-Info "Installed release tag: $($script:ReleaseTag)"
    } else {
        Write-Info "Installed release tag: not set"
    }

    if ($latestTag) {
        Write-Info "Latest release tag: $latestTag"
        if (-not $script:ReleaseTag) {
            Write-Info "No release tag is stored locally yet."
        } elseif ((Compare-VersionStrings $latestTag $script:ReleaseTag) -gt 0) {
            Write-Info "A newer binary release is available."
            if (Confirm-Choice "Update application binaries now?" "Y") {
                Update-ApplicationsToTag -TargetTag $latestTag
            }
        } else {
            Write-Info "Binary release is up to date."
        }
    } else {
        Write-Info "Latest release tag: unavailable"
    }
}

function Remove-LauncherOnly {
    $binDir = Join-Path $script:InstallDir "bin"
    $launcherFiles = @(
        (Join-Path $binDir "vldb.cmd"),
        (Join-Path $binDir "vldb.ps1")
    )

    $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $updated = ""
    if ($currentPath) {
        $updated = ($currentPath.Split(";") | Where-Object { $_ -and $_ -ne $binDir }) -join ";"
    }

    [Environment]::SetEnvironmentVariable("Path", $updated, "User")
    Clear-UserEnvironmentValue -Name "VULCANLOCALDB_HOME" -ExpectedValue $script:InstallDir
    Clear-UserEnvironmentValue -Name "VULCANLOCALDB_BIN" -ExpectedValue $binDir
    Refresh-CurrentSessionEnvironment
    Remove-Item $launcherFiles -Force -ErrorAction SilentlyContinue
    Start-DeferredCleanup -Paths $launcherFiles
    Write-Info "The vldb manager command has been removed from future sessions. Close this shell after you finish."
}

function Uninstall-All {
    foreach ($file in Get-InstanceFiles) {
        $meta = Get-InstanceMeta $file
        $serviceName = Get-ServiceRegistrationName -Service $meta.service -Instance $meta.instance -ConfigPath $file.FullName
        if (Test-RegisteredByName $serviceName) {
            Unregister-Instance -Service $meta.service -Instance $meta.instance -RegisteredName $serviceName
        }
    }

    Remove-LauncherOnly
    Start-DeferredCleanup -Paths @(
        $script:InstallDir,
        $script:RunDir,
        (Join-Path $script:GlobalHome "logs"),
        $script:GlobalConfig
    )
    Write-Info "VulcanLocalDB program files are being removed in the background."
    Write-Info "Database directories were preserved."
    exit 0
}

function Show-Menu {
    Write-Panel -Title "VulcanLocalDB Manager Script"
    Write-ColorLine -Message "0. Exit" -Color White
    Write-MenuSeparator
    Write-ColorLine -Message "1. Check for updates" -Color White
    Write-ColorLine -Message "2. Show installed instances" -Color White
    Write-ColorLine -Message "3. Modify host, port, data path or service name" -Color White
    Write-ColorLine -Message "4. Install a single service instance" -Color White
    Write-ColorLine -Message "5. Start a single service instance" -Color White
    Write-ColorLine -Message "6. Stop a single service instance" -Color White
    Write-ColorLine -Message "7. Start all service instances" -Color White
    Write-ColorLine -Message "8. Stop all service instances" -Color White
    Write-ColorLine -Message "9. Uninstall a single service instance" -Color White
    Write-ColorLine -Message "10. Remove only the vldb manager command" -Color White
    Write-ColorLine -Message "11. Uninstall everything" -Color White
}

Resolve-InstallDir
Ensure-CmdLauncher

if (-not (Test-Path (Join-Path $script:InstallDir "config"))) {
    New-Item -ItemType Directory -Force -Path (Join-Path $script:InstallDir "config") | Out-Null
}

if ((Get-InstanceFiles).Count -gt 0) {
    $script:Initialized = $true
}

$script:InstalledScriptVersion = $ScriptVersion
Write-Config

if (-not (Is-Initialized)) {
    Write-Info "No initialized application installation was detected."
    Write-Running "initial one-click installation"
    try {
        Initialize-Installation
        Write-Done "Initial one-click installation"
    } catch {
        Write-Warn ("Initial one-click installation failed: " + $_.Exception.Message)
        throw
    }
} elseif ($FromInstaller) {
    Write-Info "Installer detected an existing installation."
    Write-Running "update check"
    try {
        Check-Updates
        Write-Done "Update check"
    } catch {
        Write-Warn ("Update check failed: " + $_.Exception.Message)
    }
}

while ($true) {
    Show-Menu
    $choice = Read-Host "Select an action"
    switch ($choice) {
        "1" { Invoke-MenuAction -Label "checking for updates" -Action { Check-Updates } }
        "2" { Invoke-MenuAction -Label "showing installed instances" -Action { Show-Instances } }
        "3" { Invoke-MenuAction -Label "updating instance settings" -Action { Configure-Instance } }
        "4" { Invoke-MenuAction -Label "installing a single service instance" -Action { Install-SingleInstance } }
        "5" { Invoke-MenuAction -Label "starting a single service instance" -Action { Start-SingleInstance } }
        "6" { Invoke-MenuAction -Label "stopping a single service instance" -Action { Stop-SingleInstance } }
        "7" { Invoke-MenuAction -Label "starting all service instances" -Action { Start-AllInstances } }
        "8" { Invoke-MenuAction -Label "stopping all service instances" -Action { Stop-AllInstances } }
        "9" { Invoke-MenuAction -Label "uninstalling a single service instance" -Action { Uninstall-SingleInstance } }
        "10" { Invoke-MenuAction -Label "removing the manager command" -Action { Remove-LauncherOnly } }
        "11" { Invoke-MenuAction -Label "uninstalling everything" -Action { Uninstall-All } }
        "0" { Write-Done "Exit"; exit 0 }
        default { Write-Warn "Invalid selection." }
    }
}
