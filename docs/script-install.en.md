# Script Installer Guide

## Overview

VulcanLocalDB ships interactive installer scripts that can be downloaded directly from GitHub and run locally.

Use the script installer when you want:

- a guided setup flow instead of manual archive extraction
- automatic download of the correct release archives
- default config generation for `vldb-lancedb` and `vldb-duckdb`
- optional service registration
- the `vldb` manager command installed on the local machine

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
curl -fsSL https://raw.githubusercontent.com/OpenVulcan/vulcan-local-db/main/scripts/install.sh | bash
```

### macOS

```bash
curl -fsSL https://raw.githubusercontent.com/OpenVulcan/vulcan-local-db/main/scripts/install.sh | bash
```

### Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/OpenVulcan/vulcan-local-db/main/scripts/install.ps1 | iex
```

If your network path or proxy serves a stale cached copy of the `main` branch script, replace `main` in the URL with a known commit SHA from the repository.

## What The Installer Does

The installer can:

- show the current installer version and latest release tag
- install the full stack or only the manager script
- choose the install directory
- configure host and default ports
- download the matching GitHub Release archives
- install `vldb-lancedb` and `vldb-duckdb`
- generate default config files
- install the `vldb` manager command
- optionally register services for auto-start and auto-restart
- separate LanceDB and DuckDB data roots outside the installation directory by default

The installer stores global settings here:

- Linux and macOS: `~/.vulcan/vldb/config.json`
- Windows: `%USERPROFILE%\.vulcan\vldb\config.json`

The config file records:

- selected language
- installation directory
- installed release tag
- installed manager script version
- default LanceDB data root
- default DuckDB data root

## Dependency Handling

The installer will prompt before installing missing dependencies.

Typical dependencies:

- Linux: `curl`, `tar`, `sha256sum` or equivalent
- macOS: `curl`, `tar`, `shasum` or equivalent
- Windows: built-in PowerShell download and hash features
- Windows service mode: WinSW is downloaded automatically after confirmation

## After Installation

The manager command is:

- Linux and macOS: `vldb`
- Windows: `vldb.cmd`

Examples:

Linux or macOS:

```bash
vldb
```

Windows:

```powershell
vldb.cmd
```

The `VulcanLocalDB Manager Script` can manage installed instances, adjust IP, port, and data path values, register or unregister services, check updates, and remove instances without deleting preserved database files.

## Related Guides

- Native binary archive install: [native-install.en.md](./native-install.en.md)
- Docker quick install: [docker-install.en.md](./docker-install.en.md)
- LanceDB service guide: [vldb-lancedb.en.md](./vldb-lancedb.en.md)
- DuckDB service guide: [vldb-duckdb.en.md](./vldb-duckdb.en.md)
