# Script Installer Guide

## Overview

VulcanLocalDB ships interactive installer scripts that can be downloaded directly from GitHub and run locally.

Use the script installer when you want:

- a guided setup flow instead of manual archive extraction
- automatic download of the correct release archives
- first-time setup to be handed off to the local `vldb` manager
- default config generation for `vldb-lancedb` and `vldb-sqlite`
- automatic service registration on supported platforms
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

The installer and manager together can:

- show the current installer version
- install only the manager script first
- choose the install directory
- refresh an older local manager first when the bundled manager is newer
- launch the local manager automatically after the manager script is installed
- configure bind IP, ports, data paths, and service names during first-time setup
- download the matching GitHub Release archives from the dedicated service repositories
- install `vldb-lancedb` and `vldb-sqlite`
- generate default config files
- install the `vldb` manager command
- update common shell profile files on Linux and macOS so `vldb` is easier to use in new terminals
- register services automatically for auto-start and auto-restart on supported platforms
- separate LanceDB and SQLite data roots outside the installation directory by default

The installer stores global settings here:

- Linux and macOS: `~/.vulcan/vldb/config.json`
- Windows: `%USERPROFILE%\.vulcan\vldb\config.json`

The config file records:

- selected language
- installation directory
- installed LanceDB release tag
- installed SQLite release tag
- installed manager script version
- default LanceDB data root
- default SQLite data root

## Dependency Handling

The installer will prompt before installing missing dependencies.

Typical dependencies:

- Linux: `curl`, `tar`, `sha256sum` or equivalent
- macOS: `curl`, `tar`, `shasum` or equivalent
- Windows: built-in PowerShell download and hash features
- Windows service mode: WinSW is downloaded automatically into the `tools` directory when needed

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

The `VulcanLocalDB Manager Script` can manage installed instances, adjust IP, port, data path, and service name values, start or stop single instances, start or stop all instances, check manager and application updates, and remove instances without deleting preserved database files.

## Related Guides

- Native binary archive install: [native-install.en.md](./native-install.en.md)
- Docker quick install: [docker-install.en.md](./docker-install.en.md)
- LanceDB service guide: [OpenVulcan/vldb-lancedb](https://github.com/OpenVulcan/vldb-lancedb/blob/main/docs/README.en.md)
- SQLite service guide: [OpenVulcan/vldb-sqlite](https://github.com/OpenVulcan/vldb-sqlite/blob/main/docs/README.en.md)
