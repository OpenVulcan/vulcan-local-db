# Script Installer Guide

## Overview

VulcanLocalDB ships interactive installer scripts that can be downloaded directly from GitHub and run locally.

Use the script installer when you want:

- a guided setup flow instead of manual archive extraction
- automatic download of the correct release archives
- default config generation for `vldb-lancedb` and `vldb-duckdb`
- optional service registration
- the `vldg` management command installed on the local machine

Repository source:

- [OpenVulcan/vulcan-local-db](https://github.com/OpenVulcan/vulcan-local-db)

## Supported Platforms

- Linux: `install.sh`
- macOS: `install.sh`
- Windows PowerShell: `install.ps1`

Notes:

- Linux and macOS installers support English and Simplified Chinese.
- Windows PowerShell currently uses English only because Windows PowerShell 5.x has poor UTF-8 handling.

## Quick Start

### Linux

```bash
curl -fsSL https://raw.githubusercontent.com/OpenVulcan/vulcan-local-db/main/scripts/install.sh -o /tmp/vulcanlocaldb-install.sh
bash /tmp/vulcanlocaldb-install.sh
```

### macOS

```bash
curl -fsSL https://raw.githubusercontent.com/OpenVulcan/vulcan-local-db/main/scripts/install.sh -o /tmp/vulcanlocaldb-install.sh
bash /tmp/vulcanlocaldb-install.sh
```

### Windows PowerShell

```powershell
$installer = Join-Path $env:TEMP "VulcanLocalDB-install.ps1"
Invoke-WebRequest -UseBasicParsing https://raw.githubusercontent.com/OpenVulcan/vulcan-local-db/main/scripts/install.ps1 -OutFile $installer
powershell -NoProfile -ExecutionPolicy Bypass -File $installer
```

## What The Installer Does

The installer can:

- show the current installer version and latest release tag
- install the full stack or only the controller script
- choose the install directory
- configure host and default ports
- download the matching GitHub Release archives
- install `vldb-lancedb` and `vldb-duckdb`
- generate default config files
- install the `vldg` management command
- optionally register services for auto-start and auto-restart

The installer stores global settings here:

- Linux and macOS: `~/.vulcan/vldg/config.json`
- Windows: `%USERPROFILE%\.vulcan\vldg\config.json`

The config file records:

- selected language
- installation directory
- installed release tag
- installed controller script version

## Dependency Handling

The installer will prompt before installing missing dependencies.

Typical dependencies:

- Linux: `curl`, `tar`, `sha256sum` or equivalent
- macOS: `curl`, `tar`, `shasum` or equivalent
- Windows: built-in PowerShell download and hash features
- Windows service mode: WinSW is downloaded automatically after confirmation

## After Installation

The controller command is:

- Linux and macOS: `vldg`
- Windows: `vldg.cmd`

Examples:

Linux or macOS:

```bash
vldg
```

Windows:

```powershell
vldg.cmd
```

The controller can manage installed instances, adjust IP and port values, register or unregister services, check updates, and remove instances.

## Related Guides

- Native binary archive install: [native-install.en.md](./native-install.en.md)
- Docker quick install: [docker-install.en.md](./docker-install.en.md)
- LanceDB service guide: [vldb-lancedb.en.md](./vldb-lancedb.en.md)
- DuckDB service guide: [vldb-duckdb.en.md](./vldb-duckdb.en.md)
