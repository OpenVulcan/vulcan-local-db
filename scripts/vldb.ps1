param(
    [switch]$FromInstaller
)

$ErrorActionPreference = "Stop"

$ScriptVersion = "0.1.6"
$GlobalHome = Join-Path $HOME ".vulcan\vldb"
$GlobalConfig = Join-Path $GlobalHome "config.json"
$RunDir = Join-Path $GlobalHome "run"
$LanceDbRoot = Join-Path $GlobalHome "lancedb"
$DuckDbRoot = Join-Path $GlobalHome "duckdb"
$InstallDir = $null
$RepoSlug = "OpenVulcan/vulcan-local-db"
$RepoUrl = "https://github.com/OpenVulcan/vulcan-local-db"
$RawBaseUrl = "https://raw.githubusercontent.com/$RepoSlug/main/scripts"
$ReleaseTag = $null
$LatestRelease = $null
$InstalledScriptVersion = $ScriptVersion
$WinSWVersion = "v2.12.0"

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

function Read-Config {
    if (Test-Path $script:GlobalConfig) {
        return Get-Content $script:GlobalConfig -Raw | ConvertFrom-Json
    }
    return $null
}

function Write-Config {
    New-Item -ItemType Directory -Force -Path $script:GlobalHome | Out-Null
    @{
        language = "en"
        install_dir = $script:InstallDir
        release_tag = $script:ReleaseTag
        script_version = $ScriptVersion
        lancedb_root = $script:LanceDbRoot
        duckdb_root = $script:DuckDbRoot
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 $script:GlobalConfig
    $script:InstalledScriptVersion = $ScriptVersion
}

function Resolve-InstallDir {
    $config = Read-Config
    if ($config -and $config.install_dir) {
        $script:InstallDir = $config.install_dir
    } else {
        $script:InstallDir = Split-Path -Parent (Split-Path -Parent $PSCommandPath)
    }

    if ($config -and $config.release_tag) {
        $script:ReleaseTag = $config.release_tag
    }
    if ($config -and $config.script_version) {
        $script:InstalledScriptVersion = $config.script_version
    }
    if ($config -and $config.lancedb_root) {
        $script:LanceDbRoot = $config.lancedb_root
    }
    if ($config -and $config.duckdb_root) {
        $script:DuckDbRoot = $config.duckdb_root
    }
}

function Get-DefaultDataRoot {
    param([string]$Service)

    if ($Service -eq "vldb-lancedb") {
        return (Join-Path $script:GlobalHome "lancedb")
    }
    return (Join-Path $script:GlobalHome "duckdb")
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

function Get-LatestRelease {
    if ($script:ReleaseTag) {
        try {
            $script:LatestRelease = Invoke-RestMethod -Uri "https://api.github.com/repos/$RepoSlug/releases/tags/$ReleaseTag"
            return $script:LatestRelease
        } catch {
        }
    }

    $script:LatestRelease = Invoke-RestMethod -Uri "https://api.github.com/repos/$RepoSlug/releases/latest"
    $script:ReleaseTag = $script:LatestRelease.tag_name
    return $script:LatestRelease
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

function Check-Updates {
    Write-Info "Checking for updates..."

    $remoteScriptVersion = Get-RemoteScriptVersion -ScriptName "vldb.ps1"
    $latestTag = $null
    try {
        $latestTag = (Invoke-RestMethod -Uri "https://api.github.com/repos/$RepoSlug/releases/latest").tag_name
    } catch {
    }

    Write-Info "Current manager script version: $ScriptVersion"
    if ($remoteScriptVersion) {
        Write-Info "Latest manager script version: $remoteScriptVersion"
        if ((Compare-VersionStrings $remoteScriptVersion $ScriptVersion) -gt 0) {
            Write-Info "Manager script update available."
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
        } else {
            Write-Info "Binary release is up to date."
        }
    } else {
        Write-Info "Latest release tag: unavailable"
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

function Get-TargetTriple {
    $arch = $env:PROCESSOR_ARCHITECTURE
    switch ($arch) {
        "AMD64" { return "x86_64-pc-windows-msvc" }
        "ARM64" { return "aarch64-pc-windows-msvc" }
        default { throw "Unsupported Windows CPU architecture." }
    }
}

function Ensure-ServiceBinaryInstalled {
    param([string]$Service)

    $binaryPath = Join-Path $script:InstallDir "bin\$Service.exe"
    if (Test-Path $binaryPath) {
        return
    }

    $release = Get-LatestRelease
    $target = Get-TargetTriple
    $archiveName = "$Service-$($script:ReleaseTag)-$target.zip"
    $checksumName = "$archiveName.sha256"
    if ($release.assets.name -notcontains $archiveName) {
        throw "The current release does not provide $archiveName."
    }

    $tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("vulcanlocaldb-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Force -Path $tempDir | Out-Null

    try {
        $archivePath = Join-Path $tempDir $archiveName
        $checksumPath = Join-Path $tempDir $checksumName
        $baseUrl = "$RepoUrl/releases/download/$($script:ReleaseTag)"

        Download-FileWithProgress -Url "$baseUrl/$archiveName" -OutFile $archivePath -Label $archiveName
        Download-FileWithProgress -Url "$baseUrl/$checksumName" -OutFile $checksumPath -Label $checksumName

        $expected = (Get-Content $checksumPath -Raw).Split(" ")[0].Trim().ToLowerInvariant()
        $actual = (Get-FileHash -Algorithm SHA256 $archivePath).Hash.ToLowerInvariant()
        if ($expected -ne $actual) {
            throw "Checksum verification failed for $archiveName."
        }

        $extractDir = Join-Path $tempDir "extract-$Service"
        Expand-Archive -Path $archivePath -DestinationPath $extractDir -Force
        $binary = Get-ChildItem -Path $extractDir -Recurse -File -Filter "$Service.exe" | Select-Object -First 1
        $example = Get-ChildItem -Path $extractDir -Recurse -File -Filter "$Service.json.example" | Select-Object -First 1

        if (-not $binary -or -not $example) {
            throw "The archive layout is missing the expected binary or example config."
        }

        New-Item -ItemType Directory -Force -Path (Join-Path $script:InstallDir "bin"), (Join-Path $script:InstallDir "share\examples") | Out-Null
        Copy-Item $binary.FullName $binaryPath -Force
        Copy-Item $example.FullName (Join-Path $script:InstallDir "share\examples\$Service.json.example") -Force
    } finally {
        Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
    }
}

function Get-InstanceFiles {
    $configDir = Join-Path $script:InstallDir "config"
    if (-not (Test-Path $configDir)) { return @() }
    return Get-ChildItem -Path $configDir -File | Where-Object {
        $_.Name -like "vldb-lancedb-*.json" -or $_.Name -like "vldb-duckdb-*.json"
    } | Sort-Object Name
}

function Get-ConfigDbPath {
    param([string]$Path)

    if (-not (Test-Path $Path)) { return $null }
    try {
        return (Get-Content $Path -Raw | ConvertFrom-Json).db_path
    } catch {
        return $null
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

function Choose-Service {
    while ($true) {
        Write-Host "1. LanceDB"
        Write-Host "2. DuckDB"
        $choice = Read-Host "Choose service [1/2]"
        switch ($choice) {
            "1" { return "vldb-lancedb" }
            "2" { return "vldb-duckdb" }
            default { Write-Info "Please input 1 or 2." }
        }
    }
}

function Choose-InstanceFile {
    param([string]$ServiceFilter = "")

    $files = Get-InstanceFiles
    if ($ServiceFilter) {
        $files = $files | Where-Object { $_.BaseName -like "$ServiceFilter-*" }
    }
    if (-not $files -or $files.Count -eq 0) {
        Write-Info "No installed instances were found."
        return $null
    }

    for ($i = 0; $i -lt $files.Count; $i++) {
        Write-Host ("{0}. {1}" -f ($i + 1), $files[$i].BaseName)
    }

    while ($true) {
        $choice = Read-Host "Select instance"
        if ($choice -match '^\d+$') {
            $index = [int]$choice - 1
            if ($index -ge 0 -and $index -lt $files.Count) {
                return $files[$index]
            }
        }
        Write-Info "Invalid selection."
    }
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

function Write-ServiceConfig {
    param([string]$Service, [string]$Instance, [string]$BindHost, [int]$Port, [string]$DataPath)

    $configDir = Join-Path $script:InstallDir "config"
    New-Item -ItemType Directory -Force -Path $configDir | Out-Null

    if ($Service -eq "vldb-lancedb") {
        New-Item -ItemType Directory -Force -Path $DataPath | Out-Null
        @{
            host = $BindHost
            port = $Port
            db_path = $DataPath
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
        } | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 (Get-InstanceConfigPath $Service $Instance)
    }
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
    $jsonConfig = Get-InstanceConfigPath -Service $Service -Instance $Instance
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

function Test-Registered {
    param([string]$Service, [string]$Instance)
    return $null -ne (Get-Service -Name (Get-ServiceName -Service $Service -Instance $Instance) -ErrorAction SilentlyContinue)
}

function Register-Instance {
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

function Unregister-Instance {
    param([string]$Service, [string]$Instance)

    $wrapperExe = Get-ServiceWrapperExePath -Service $Service -Instance $Instance
    Remove-LegacyStartupTask -Service $Service -Instance $Instance

    if (Test-Path $wrapperExe) {
        & $wrapperExe stop 2>$null
        & $wrapperExe uninstall 2>$null
    }

    Remove-Item (Get-ServiceWrapperDir -Service $Service -Instance $Instance) -Recurse -Force -ErrorAction SilentlyContinue
}

function Show-Instances {
    $files = Get-InstanceFiles
    if (-not $files -or $files.Count -eq 0) {
        Write-Info "No installed instances were found."
        return
    }

    foreach ($file in $files) {
        $meta = Get-InstanceMeta $file
        $cfg = Get-Content $file.FullName -Raw | ConvertFrom-Json
        $registered = if (Test-Registered $meta.service $meta.instance) { "registered" } else { "not registered" }
        Write-Host ("{0} {1} | {2}:{3} | {4} | {5}" -f $meta.service, $meta.instance, $cfg.host, $cfg.port, $registered, $cfg.db_path)
    }
}

function Configure-Instance {
    $file = Choose-InstanceFile
    if (-not $file) { return }
    $meta = Get-InstanceMeta $file
    $cfg = Get-Content $file.FullName -Raw | ConvertFrom-Json

    while ($true) {
        $bindHost = Read-Host ("Bind IP [{0}]" -f $cfg.host)
        if ([string]::IsNullOrWhiteSpace($bindHost)) { $bindHost = $cfg.host }

        $portInput = Read-Host ("Port [{0}]" -f $cfg.port)
        if ([string]::IsNullOrWhiteSpace($portInput)) { $portInput = "$($cfg.port)" }
        if (-not ($portInput -match '^\d+$' -and [int]$portInput -ge 1 -and [int]$portInput -le 65535)) {
            Write-Info "Invalid port."
            continue
        }

        $dataPathInput = Read-Host ("Data path [{0}]" -f $cfg.db_path)
        if ([string]::IsNullOrWhiteSpace($dataPathInput)) { $dataPathInput = $cfg.db_path }
        $validationError = Get-DataPathValidationError -CandidatePath $dataPathInput -Service $meta.service -Instance $meta.instance
        if ($validationError) {
            Write-Info $validationError
            continue
        }

        $port = [int]$portInput
        $dataPath = Resolve-NormalizedPath $dataPathInput
        break
    }

    Write-ServiceConfig -Service $meta.service -Instance $meta.instance -BindHost $bindHost -Port $port -DataPath $dataPath
    Write-Config
    if (Test-Registered $meta.service $meta.instance) {
        Register-Instance -Service $meta.service -Instance $meta.instance
    }
}

function Install-SingleInstance {
    $service = Choose-Service
    Ensure-ServiceBinaryInstalled -Service $service
    Write-Config

    while ($true) {
        $instance = Read-Host "Instance name [default]"
        if ([string]::IsNullOrWhiteSpace($instance)) { $instance = "default" }
        if ($instance -match '^[A-Za-z0-9][A-Za-z0-9_-]{0,31}$') {
            if (-not (Test-Path (Get-InstanceConfigPath $service $instance))) { break }
            Write-Info "This instance already exists."
        } else {
            Write-Info "Instance names may contain letters, numbers, dash, and underscore."
        }
    }

    while ($true) {
        $bindHost = Read-Host "Bind IP [127.0.0.1]"
        if ([string]::IsNullOrWhiteSpace($bindHost)) { $bindHost = "127.0.0.1" }

        $defaultPort = if ($service -eq "vldb-lancedb") { 50051 } else { 50052 }
        $portInput = Read-Host ("Port [{0}]" -f $defaultPort)
        if ([string]::IsNullOrWhiteSpace($portInput)) { $portInput = "$defaultPort" }
        if (-not ($portInput -match '^\d+$' -and [int]$portInput -ge 1 -and [int]$portInput -le 65535)) {
            Write-Info "Invalid port."
            continue
        }

        $defaultDataPath = Get-DefaultInstanceDataPath -Service $service -Instance $instance
        $dataPathInput = Read-Host ("Data path [{0}]" -f $defaultDataPath)
        if ([string]::IsNullOrWhiteSpace($dataPathInput)) { $dataPathInput = $defaultDataPath }
        $validationError = Get-DataPathValidationError -CandidatePath $dataPathInput -Service $service -Instance $instance
        if ($validationError) {
            Write-Info $validationError
            continue
        }

        $port = [int]$portInput
        $dataPath = Resolve-NormalizedPath $dataPathInput
        break
    }

    Write-ServiceConfig -Service $service -Instance $instance -BindHost $bindHost -Port $port -DataPath $dataPath
    Write-Config
    if (Confirm-Choice "Register this instance as a service now?" "N") {
        Register-Instance -Service $service -Instance $instance
    }
}

function Uninstall-SingleInstance {
    $file = Choose-InstanceFile
    if (-not $file) { return }
    $meta = Get-InstanceMeta $file
    $cfg = Get-Content $file.FullName -Raw | ConvertFrom-Json

    if (Test-Registered $meta.service $meta.instance) {
        Unregister-Instance -Service $meta.service -Instance $meta.instance
    }

    Remove-Item $file.FullName -Force
    Write-Info ("Database files were preserved at: {0}" -f $cfg.db_path)
}

function Toggle-ServiceRegistration {
    $file = Choose-InstanceFile
    if (-not $file) { return }
    $meta = Get-InstanceMeta $file
    if (Test-Registered $meta.service $meta.instance) {
        Unregister-Instance -Service $meta.service -Instance $meta.instance
    } else {
        Register-Instance -Service $meta.service -Instance $meta.instance
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
        if (Test-Registered $meta.service $meta.instance) {
            Unregister-Instance -Service $meta.service -Instance $meta.instance
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
    Write-Host ""
    Write-Host "===================================="
    Write-Host "VulcanLocalDB Manager Script"
    Write-Host "===================================="
    Write-Host "1. Check for updates"
    Write-Host "2. Show installed instances"
    Write-Host "3. Modify host, port or data path"
    Write-Host "4. Install a single service instance"
    Write-Host "5. Uninstall a single service instance"
    Write-Host "6. Register or unregister a service instance"
    Write-Host "7. Remove only the vldb manager command"
    Write-Host "8. Uninstall everything"
    Write-Host "0. Exit"
}

Resolve-InstallDir
Write-Config

if ($FromInstaller) {
    Write-Info "Installer detected an existing installation. Running an update check first."
    Check-Updates
}

while ($true) {
    Show-Menu
    $choice = Read-Host "Select an action"
    switch ($choice) {
        "1" { Check-Updates }
        "2" { Show-Instances }
        "3" { Configure-Instance }
        "4" { Install-SingleInstance }
        "5" { Uninstall-SingleInstance }
        "6" { Toggle-ServiceRegistration }
        "7" { Remove-LauncherOnly }
        "8" { Uninstall-All }
        "0" { break }
        default { Write-Info "Invalid selection." }
    }
}
