#!/usr/bin/env bash
set -euo pipefail

SCRIPT_VERSION="0.1.0"
REPO_SLUG="OpenVulcan/vulcan-local-db"
REPO_URL="https://github.com/OpenVulcan/vulcan-local-db"
RAW_BASE_URL="https://raw.githubusercontent.com/${REPO_SLUG}/main/scripts"
GLOBAL_HOME="${HOME}/.vulcan/vldg"
GLOBAL_CONFIG="${GLOBAL_HOME}/config.json"
RUNNER_DIR="${GLOBAL_HOME}/run"
LANG_CODE="en"
INSTALL_DIR=""
INSTALL_TAG=""
HOST_BIND="127.0.0.1"
LANCEDB_PORT="50051"
DUCKDB_PORT="50052"
LATEST_RELEASE_JSON=""
INSTALL_MODE="full"
CONTROLLER_SCRIPT_VERSION="${SCRIPT_VERSION}"

say() {
  if [[ "${LANG_CODE}" == "zh-CN" ]]; then
    printf '%s' "$2"
  else
    printf '%s' "$1"
  fi
}

line() {
  printf '%s\n' "$(say "$1" "$2")"
}

prompt_default() {
  local prompt_en="$1"
  local prompt_zh="$2"
  local default_value="$3"
  local answer

  read -r -p "$(say "$prompt_en [$default_value]: " "$prompt_zh [$default_value]: ")" answer
  if [[ -z "$answer" ]]; then
    printf '%s' "$default_value"
  else
    printf '%s' "$answer"
  fi
}

confirm_yes_no() {
  local prompt_en="$1"
  local prompt_zh="$2"
  local default_answer="${3:-Y}"
  local answer
  local normalized_default

  if [[ "${default_answer}" =~ ^[Nn]$ ]]; then
    normalized_default="N"
  else
    normalized_default="Y"
  fi

  while true; do
    read -r -p "$(say "$prompt_en [$normalized_default]: " "$prompt_zh [$normalized_default]: ")" answer
    answer="${answer:-$normalized_default}"
    case "${answer}" in
      [Yy]) return 0 ;;
      [Nn]) return 1 ;;
      *) line "Please input Y or N." "请输入 Y 或 N。" ;;
    esac
  done
}

normalize_version() {
  local value="${1:-}"
  value="${value#v}"
  printf '%s' "${value}"
}

version_compare() {
  local left right
  local -a left_parts=() right_parts=()
  local max_count=0
  local index
  local left_value right_value

  left="$(normalize_version "$1")"
  right="$(normalize_version "$2")"

  if [[ -z "${left}" && -z "${right}" ]]; then
    printf '0'
    return
  fi
  if [[ -z "${left}" ]]; then
    printf -- '-1'
    return
  fi
  if [[ -z "${right}" ]]; then
    printf '1'
    return
  fi

  IFS=. read -r -a left_parts <<<"${left}"
  IFS=. read -r -a right_parts <<<"${right}"

  if (( ${#left_parts[@]} > ${#right_parts[@]} )); then
    max_count=${#left_parts[@]}
  else
    max_count=${#right_parts[@]}
  fi

  for (( index = 0; index < max_count; index += 1 )); do
    left_value="${left_parts[index]:-0}"
    right_value="${right_parts[index]:-0}"
    left_value="${left_value//[^0-9]/}"
    right_value="${right_value//[^0-9]/}"
    left_value="${left_value:-0}"
    right_value="${right_value:-0}"

    if (( 10#${left_value} > 10#${right_value} )); then
      printf '1'
      return
    fi
    if (( 10#${left_value} < 10#${right_value} )); then
      printf -- '-1'
      return
    fi
  done

  printf '0'
}

PACKAGE_MANAGER=""

resolve_package_manager() {
  if [[ -n "${PACKAGE_MANAGER}" ]]; then
    printf '%s' "${PACKAGE_MANAGER}"
    return 0
  fi

  if command -v apt-get >/dev/null 2>&1; then
    PACKAGE_MANAGER="apt-get"
  elif command -v dnf >/dev/null 2>&1; then
    PACKAGE_MANAGER="dnf"
  elif command -v yum >/dev/null 2>&1; then
    PACKAGE_MANAGER="yum"
  elif command -v zypper >/dev/null 2>&1; then
    PACKAGE_MANAGER="zypper"
  elif command -v pacman >/dev/null 2>&1; then
    PACKAGE_MANAGER="pacman"
  elif command -v apk >/dev/null 2>&1; then
    PACKAGE_MANAGER="apk"
  elif command -v brew >/dev/null 2>&1; then
    PACKAGE_MANAGER="brew"
  else
    return 1
  fi

  printf '%s' "${PACKAGE_MANAGER}"
}

run_with_privilege() {
  if [[ "$1" == "brew" || "$(id -u)" -eq 0 ]]; then
    "$@"
  elif command -v sudo >/dev/null 2>&1; then
    sudo "$@"
  else
    line "This step needs administrator privileges. Please run as root or install sudo first." "这一步需要管理员权限。请以 root 身份运行，或先安装 sudo。"
    return 1
  fi
}

install_packages() {
  local manager
  manager="$(resolve_package_manager)" || {
    line "No supported package manager was found. Please install the dependency manually and run the script again." "未找到受支持的软件包管理器。请先手动安装依赖后再运行脚本。"
    return 1
  }

  case "${manager}" in
    apt-get)
      run_with_privilege apt-get update
      run_with_privilege apt-get install -y "$@"
      ;;
    dnf)
      run_with_privilege dnf install -y "$@"
      ;;
    yum)
      run_with_privilege yum install -y "$@"
      ;;
    zypper)
      run_with_privilege zypper --non-interactive install "$@"
      ;;
    pacman)
      run_with_privilege pacman -Sy --noconfirm "$@"
      ;;
    apk)
      run_with_privilege apk add --no-cache "$@"
      ;;
    brew)
      brew install "$@"
      ;;
  esac
}

ensure_command() {
  local command_name="$1"
  shift || true
  local packages=("$@")

  if command -v "${command_name}" >/dev/null 2>&1; then
    return 0
  fi

  if [[ "${#packages[@]}" -eq 0 ]]; then
    packages=("${command_name}")
  fi

  line "Missing required command: ${command_name}" "缺少必需命令：${command_name}"
  if ! confirm_yes_no "Install the missing dependency now?" "现在安装缺少的依赖？" "Y"; then
    exit 1
  fi

  install_packages "${packages[@]}"
  if ! command -v "${command_name}" >/dev/null 2>&1; then
    line "The dependency was not installed successfully: ${command_name}" "依赖安装失败：${command_name}"
    exit 1
  fi
}

ensure_checksum_tool() {
  if command -v sha256sum >/dev/null 2>&1 || command -v shasum >/dev/null 2>&1 || command -v openssl >/dev/null 2>&1; then
    return 0
  fi

  line "A SHA-256 verification tool is required." "需要 SHA-256 校验工具。"
  if ! confirm_yes_no "Install the checksum dependency now?" "现在安装校验依赖？" "Y"; then
    exit 1
  fi

  case "$(uname -s)" in
    Darwin)
      install_packages perl
      ;;
    *)
      install_packages coreutils
      ;;
  esac

  if ! command -v sha256sum >/dev/null 2>&1 && ! command -v shasum >/dev/null 2>&1 && ! command -v openssl >/dev/null 2>&1; then
    line "No checksum tool is available after installation." "安装后仍然没有可用的校验工具。"
    exit 1
  fi
}

detect_profile_file() {
  if [[ -n "${PROFILE:-}" ]]; then
    printf '%s' "${PROFILE}"
    return
  fi

  case "${SHELL:-}" in
    */zsh)
      printf '%s' "${HOME}/.zprofile"
      ;;
    */bash)
      if [[ -f "${HOME}/.bash_profile" ]]; then
        printf '%s' "${HOME}/.bash_profile"
      else
        printf '%s' "${HOME}/.profile"
      fi
      ;;
    *)
      printf '%s' "${HOME}/.profile"
      ;;
  esac
}

default_install_dir() {
  case "$(uname -s)" in
    Darwin)
      if [[ "$(id -u)" -eq 0 ]]; then
        printf '%s' "/Applications/VulcanLocalDB"
      else
        printf '%s' "${HOME}/Applications/VulcanLocalDB"
      fi
      ;;
    *)
      if [[ "$(id -u)" -eq 0 ]]; then
        printf '%s' "/opt/VulcanLocalDB"
      else
        printf '%s' "${HOME}/.local/share/VulcanLocalDB"
      fi
      ;;
  esac
}

is_valid_install_dir() {
  local value="$1"

  [[ -n "$value" ]] || return 1
  [[ "$value" = /* ]] || return 1
  [[ "$value" != *$'\n'* ]] || return 1
  [[ "$value" != *$'\r'* ]] || return 1
  [[ "$value" != *'"'* ]] || return 1
  return 0
}

is_valid_port() {
  local value="$1"

  [[ "$value" =~ ^[0-9]+$ ]] || return 1
  (( value >= 1 && value <= 65535 ))
}

extract_script_version_from_file() {
  local file="$1"
  sed -nE 's/^SCRIPT_VERSION="([^"]+)".*/\1/p' "${file}" | head -n1
}

try_fetch_remote_script_version() {
  local script_name="$1"
  local content

  if ! command -v curl >/dev/null 2>&1; then
    return 1
  fi

  content="$(curl -fsSL "${RAW_BASE_URL}/${script_name}" 2>/dev/null || true)"
  [[ -n "${content}" ]] || return 1
  printf '%s\n' "${content}" | sed -nE 's/^SCRIPT_VERSION="([^"]+)".*/\1/p' | head -n1
}

try_fetch_latest_tag() {
  local payload

  if ! command -v curl >/dev/null 2>&1; then
    return 1
  fi

  payload="$(curl -fsSL "https://api.github.com/repos/${REPO_SLUG}/releases/latest" 2>/dev/null || true)"
  [[ -n "${payload}" ]] || return 1
  printf '%s\n' "${payload}" | sed -nE 's/.*"tag_name":[[:space:]]*"([^"]+)".*/\1/p' | head -n1
}

show_update_notice() {
  local remote_script_version=""
  local latest_tag=""

  remote_script_version="$(try_fetch_remote_script_version "install.sh" || true)"
  latest_tag="$(try_fetch_latest_tag || true)"

  if [[ -n "${remote_script_version}" && "$(version_compare "${remote_script_version}" "${SCRIPT_VERSION}")" == "1" ]]; then
    line "A newer installer script is available: ${remote_script_version} (current: ${SCRIPT_VERSION})." "发现更新的安装脚本版本：${remote_script_version}（当前：${SCRIPT_VERSION}）。"
  else
    line "Installer script version: ${SCRIPT_VERSION}" "安装脚本版本：${SCRIPT_VERSION}"
  fi

  if [[ -n "${latest_tag}" ]]; then
    line "Latest release tag: ${latest_tag}" "最新 release 标签：${latest_tag}"
  fi
}

choose_language() {
  printf '%s\n' "===================================="
  printf '%s\n' "       VulcanLocalDB Setup"
  printf '%s\n' "===================================="
  printf '%s\n' "1. English (default)"
  printf '%s\n' "2. 简体中文"

  local answer
  read -r -p "Select language / 选择语言 [1]: " answer

  case "${answer:-1}" in
    2) LANG_CODE="zh-CN" ;;
    *) LANG_CODE="en" ;;
  esac
}

choose_install_mode() {
  line "Install mode:" "安装模式："
  printf '%s\n' "$(say "1. Full install (services + controller)" "1. 完整安装（服务 + 管理脚本）")"
  printf '%s\n' "$(say "2. Controller only" "2. 仅安装管理脚本")"

  local answer
  while true; do
    read -r -p "$(say "Select mode [1]: " "选择模式 [1]: ")" answer
    case "${answer:-1}" in
      1) INSTALL_MODE="full"; return ;;
      2) INSTALL_MODE="controller-only"; return ;;
      *) line "Please input 1 or 2." "请输入 1 或 2。" ;;
    esac
  done
}

choose_install_dir() {
  local default_dir
  local candidate

  default_dir="$(default_install_dir)"

  while true; do
    candidate="$(prompt_default "Installation directory" "安装目录" "${default_dir}")"

    if ! is_valid_install_dir "${candidate}"; then
      line "Please use an absolute path without quotes or line breaks." "请输入合法的绝对路径，且不能包含引号或换行。"
      continue
    fi

    if [[ -e "${candidate}" && ! -d "${candidate}" ]]; then
      line "The selected path already exists and is not a directory." "所选路径已存在且不是目录。"
      continue
    fi

    mkdir -p "${candidate}" 2>/dev/null || true
    if [[ ! -d "${candidate}" ]]; then
      line "The installer cannot create or access this directory." "安装器无法创建或访问该目录。"
      continue
    fi

    printf '%s\n' "$(say "Install to: ${candidate}" "安装到：${candidate}")"
    if confirm_yes_no "Confirm this installation directory?" "确认使用该安装目录？" "Y"; then
      INSTALL_DIR="${candidate}"
      return
    fi
  done
}

choose_network_settings() {
  while true; do
    HOST_BIND="$(prompt_default "Service bind IP" "服务绑定 IP" "127.0.0.1")"
    [[ -n "${HOST_BIND}" ]] || {
      line "IP must not be empty." "IP 不能为空。"
      continue
    }

    LANCEDB_PORT="$(prompt_default "LanceDB port" "LanceDB 端口" "50051")"
    is_valid_port "${LANCEDB_PORT}" || {
      line "Invalid LanceDB port." "LanceDB 端口不合法。"
      continue
    }

    DUCKDB_PORT="$(prompt_default "DuckDB port" "DuckDB 端口" "50052")"
    is_valid_port "${DUCKDB_PORT}" || {
      line "Invalid DuckDB port." "DuckDB 端口不合法。"
      continue
    }

    if [[ "${LANCEDB_PORT}" == "${DUCKDB_PORT}" ]]; then
      line "LanceDB and DuckDB must use different ports." "LanceDB 和 DuckDB 端口不能相同。"
      continue
    fi

    return
  done
}

fetch_latest_tag() {
  ensure_command curl
  LATEST_RELEASE_JSON="$(curl -fsSL "https://api.github.com/repos/${REPO_SLUG}/releases/latest")"
  INSTALL_TAG="$(printf '%s\n' "${LATEST_RELEASE_JSON}" | sed -nE 's/.*"tag_name":[[:space:]]*"([^"]+)".*/\1/p' | head -n1)"

  if [[ -z "${INSTALL_TAG}" ]]; then
    line "Unable to resolve the latest release tag." "无法解析最新 release 标签。"
    exit 1
  fi
}

download_with_progress() {
  local url="$1"
  local output_path="$2"
  local label="$3"

  ensure_command curl
  line "Downloading ${label}" "正在下载 ${label}"
  curl --fail --location --retry 3 --retry-delay 2 --progress-bar --output "${output_path}" "${url}"
}

release_has_asset() {
  local asset_name="$1"
  grep -F "\"name\": \"${asset_name}\"" >/dev/null 2>&1 <<<"${LATEST_RELEASE_JSON}"
}

detect_target() {
  local os_name
  local arch_name

  os_name="$(uname -s)"
  arch_name="$(uname -m)"

  case "${os_name}" in
    Linux)
      case "${arch_name}" in
        x86_64|amd64) printf '%s' "x86_64-unknown-linux-gnu" ;;
        aarch64|arm64) printf '%s' "aarch64-unknown-linux-gnu" ;;
        *) return 1 ;;
      esac
      ;;
    Darwin)
      case "${arch_name}" in
        arm64|aarch64) printf '%s' "aarch64-apple-darwin" ;;
        x86_64) printf '%s' "x86_64-apple-darwin" ;;
        *) return 1 ;;
      esac
      ;;
    *)
      return 1
      ;;
  esac
}

verify_checksum() {
  local archive_path="$1"
  local checksum_path="$2"

  ensure_checksum_tool
  if command -v sha256sum >/dev/null 2>&1; then
    (cd "$(dirname "${archive_path}")" && sha256sum -c "$(basename "${checksum_path}")")
  elif command -v shasum >/dev/null 2>&1; then
    (cd "$(dirname "${archive_path}")" && shasum -a 256 -c "$(basename "${checksum_path}")")
  elif command -v openssl >/dev/null 2>&1; then
    local expected actual
    expected="$(awk '{print $1}' "${checksum_path}")"
    actual="$(openssl dgst -sha256 -r "${archive_path}" | awk '{print $1}')"
    [[ "${expected}" == "${actual}" ]]
  else
    line "Missing sha256sum/shasum for checksum verification." "缺少 sha256sum 或 shasum，无法校验文件。"
    exit 1
  fi
}

download_asset_pair() {
  local service="$1"
  local tag="$2"
  local target="$3"
  local temp_dir="$4"
  local archive_name="${service}-${tag}-${target}.tar.gz"
  local checksum_name="${archive_name}.sha256"
  local base_url="${REPO_URL}/releases/download/${tag}"
  local archive_path="${temp_dir}/${archive_name}"
  local checksum_path="${temp_dir}/${checksum_name}"

  if ! release_has_asset "${archive_name}"; then
    line "The current release does not provide ${archive_name}." "当前 release 不提供 ${archive_name}。"
    exit 1
  fi

  download_with_progress "${base_url}/${archive_name}" "${archive_path}" "${archive_name}"
  download_with_progress "${base_url}/${checksum_name}" "${checksum_path}" "${checksum_name}"
  verify_checksum "${archive_path}" "${checksum_path}"

  printf '%s\n' "${archive_path}"
}

extract_binary() {
  local archive_path="$1"
  local service="$2"
  local temp_dir="$3"
  local extract_dir="${temp_dir}/extract-${service}"
  local binary_path
  local example_path

  rm -rf "${extract_dir}"
  mkdir -p "${extract_dir}"
  tar -xzf "${archive_path}" -C "${extract_dir}"

  binary_path="$(find "${extract_dir}" -type f -name "${service}" | head -n1 || true)"
  example_path="$(find "${extract_dir}" -type f -name "${service}.json.example" | head -n1 || true)"

  if [[ -z "${binary_path}" || -z "${example_path}" ]]; then
    line "The archive layout is missing the expected binary or example config." "压缩包缺少预期的可执行文件或示例配置文件。"
    exit 1
  fi

  mkdir -p "${INSTALL_DIR}/bin" "${INSTALL_DIR}/share/examples"
  install -m 755 "${binary_path}" "${INSTALL_DIR}/bin/${service}"
  install -m 644 "${example_path}" "${INSTALL_DIR}/share/examples/${service}.json.example"
}

write_lancedb_config() {
  local instance="$1"
  local host="$2"
  local port="$3"
  local config_path="${INSTALL_DIR}/config/vldb-lancedb-${instance}.json"
  local data_path="${INSTALL_DIR}/data/lancedb/${instance}"

  mkdir -p "${INSTALL_DIR}/config" "${data_path}"
  cat >"${config_path}" <<EOF
{
  "host": "${host}",
  "port": ${port},
  "db_path": "${data_path}"
}
EOF
}

write_duckdb_config() {
  local instance="$1"
  local host="$2"
  local port="$3"
  local config_path="${INSTALL_DIR}/config/vldb-duckdb-${instance}.json"
  local data_dir="${INSTALL_DIR}/data/duckdb/${instance}"

  mkdir -p "${INSTALL_DIR}/config" "${data_dir}"
  cat >"${config_path}" <<EOF
{
  "host": "${host}",
  "port": ${port},
  "db_path": "${data_dir}/duckdb.db",
  "memory_limit": "2GB",
  "threads": 4
}
EOF
}

write_global_config() {
  mkdir -p "${GLOBAL_HOME}"
  cat >"${GLOBAL_CONFIG}" <<EOF
{
  "language": "${LANG_CODE}",
  "install_dir": "${INSTALL_DIR}",
  "release_tag": "${INSTALL_TAG}",
  "script_version": "${CONTROLLER_SCRIPT_VERSION}"
}
EOF
}

install_manager_script() {
  local source_dir
  local raw_base
  local installed_script_path

  source_dir="$(cd "$(dirname "$0")" && pwd)"
  installed_script_path="${INSTALL_DIR}/bin/vldg"
  mkdir -p "${INSTALL_DIR}/bin"

  if [[ -f "${source_dir}/vldg" ]]; then
    install -m 755 "${source_dir}/vldg" "${installed_script_path}"
  else
    raw_base="${RAW_BASE_URL}"
    download_with_progress "${raw_base}/vldg" "${installed_script_path}" "vldg"
    chmod 755 "${installed_script_path}"
  fi

  CONTROLLER_SCRIPT_VERSION="$(extract_script_version_from_file "${installed_script_path}" || true)"
  CONTROLLER_SCRIPT_VERSION="${CONTROLLER_SCRIPT_VERSION:-${SCRIPT_VERSION}}"
}

ensure_profile_exports() {
  local profile_file
  local marker_begin="# VulcanLocalDB begin"
  local marker_end="# VulcanLocalDB end"

  profile_file="$(detect_profile_file)"
  mkdir -p "$(dirname "${profile_file}")"
  touch "${profile_file}"

  if ! grep -Fq "${marker_begin}" "${profile_file}"; then
    cat >>"${profile_file}" <<EOF
${marker_begin}
export VULCANLOCALDB_HOME="${INSTALL_DIR}"
export PATH="${INSTALL_DIR}/bin:\$PATH"
${marker_end}
EOF
  fi
}

write_runner_script() {
  local service="$1"
  local instance="$2"
  local config_path="${INSTALL_DIR}/config/${service}-${instance}.json"
  local runner_path="${RUNNER_DIR}/${service}-${instance}.sh"

  mkdir -p "${RUNNER_DIR}"
  cat >"${runner_path}" <<EOF
#!/usr/bin/env bash
exec "${INSTALL_DIR}/bin/${service}" --config "${config_path}"
EOF
  chmod 755 "${runner_path}"
}

linux_unit_path() {
  local service="$1"
  local instance="$2"
  local unit_name="${service}-${instance}.service"

  if [[ "$(id -u)" -eq 0 ]]; then
    printf '%s' "/etc/systemd/system/${unit_name}"
  else
    printf '%s' "${HOME}/.config/systemd/user/${unit_name}"
  fi
}

register_linux_service() {
  local service="$1"
  local instance="$2"
  local unit_path
  local unit_name="${service}-${instance}.service"
  local runner_path="${RUNNER_DIR}/${service}-${instance}.sh"
  local wanted_by="multi-user.target"
  local systemctl_cmd=(systemctl)

  write_runner_script "${service}" "${instance}"
  unit_path="$(linux_unit_path "${service}" "${instance}")"
  mkdir -p "$(dirname "${unit_path}")"

  if [[ "$(id -u)" -ne 0 ]]; then
    wanted_by="default.target"
    systemctl_cmd+=(--user)
    if command -v loginctl >/dev/null 2>&1; then
      loginctl enable-linger "${USER}" >/dev/null 2>&1 || true
    fi
  fi

  cat >"${unit_path}" <<EOF
[Unit]
Description=${service} (${instance})
After=network.target

[Service]
Type=simple
ExecStart=${runner_path}
Restart=always
RestartSec=3
WorkingDirectory=${INSTALL_DIR}

[Install]
WantedBy=${wanted_by}
EOF

  "${systemctl_cmd[@]}" daemon-reload
  "${systemctl_cmd[@]}" enable --now "${unit_name}"
}

unregister_linux_service() {
  local service="$1"
  local instance="$2"
  local unit_name="${service}-${instance}.service"
  local unit_path
  local systemctl_cmd=(systemctl)

  unit_path="$(linux_unit_path "${service}" "${instance}")"
  [[ "$(id -u)" -eq 0 ]] || systemctl_cmd+=(--user)

  "${systemctl_cmd[@]}" disable --now "${unit_name}" >/dev/null 2>&1 || true
  rm -f "${unit_path}"
  "${systemctl_cmd[@]}" daemon-reload >/dev/null 2>&1 || true
}

launchd_plist_path() {
  local service="$1"
  local instance="$2"
  local label="com.openvulcan.${service}.${instance}"

  if [[ "$(id -u)" -eq 0 ]]; then
    printf '%s' "/Library/LaunchDaemons/${label}.plist"
  else
    printf '%s' "${HOME}/Library/LaunchAgents/${label}.plist"
  fi
}

register_macos_service() {
  local service="$1"
  local instance="$2"
  local label="com.openvulcan.${service}.${instance}"
  local plist_path
  local runner_path="${RUNNER_DIR}/${service}-${instance}.sh"

  write_runner_script "${service}" "${instance}"
  plist_path="$(launchd_plist_path "${service}" "${instance}")"
  mkdir -p "$(dirname "${plist_path}")"

  cat >"${plist_path}" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>Label</key>
    <string>${label}</string>
    <key>ProgramArguments</key>
    <array>
      <string>${runner_path}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>WorkingDirectory</key>
    <string>${INSTALL_DIR}</string>
    <key>StandardOutPath</key>
    <string>${GLOBAL_HOME}/${service}-${instance}.log</string>
    <key>StandardErrorPath</key>
    <string>${GLOBAL_HOME}/${service}-${instance}.err.log</string>
  </dict>
</plist>
EOF

  launchctl unload -w "${plist_path}" >/dev/null 2>&1 || true
  launchctl load -w "${plist_path}"
}

unregister_macos_service() {
  local service="$1"
  local instance="$2"
  local plist_path

  plist_path="$(launchd_plist_path "${service}" "${instance}")"
  launchctl unload -w "${plist_path}" >/dev/null 2>&1 || true
  rm -f "${plist_path}"
}

register_default_services() {
  case "$(uname -s)" in
    Linux)
      register_linux_service "vldb-lancedb" "default"
      register_linux_service "vldb-duckdb" "default"
      ;;
    Darwin)
      register_macos_service "vldb-lancedb" "default"
      register_macos_service "vldb-duckdb" "default"
      ;;
    *)
      line "Automatic service registration is not supported on this platform." "当前平台不支持自动服务注册。"
      ;;
  esac
}

main() {
  local target
  local temp_dir
  local lancedb_archive
  local duckdb_archive

  ensure_command tar
  choose_language
  show_update_notice
  choose_install_mode
  choose_install_dir
  if [[ "${INSTALL_MODE}" == "full" ]]; then
    choose_network_settings
    fetch_latest_tag
    target="$(detect_target)" || {
      line "Unsupported operating system or CPU architecture." "不支持当前操作系统或 CPU 架构。"
      exit 1
    }

    line "Resolved release tag: ${INSTALL_TAG}" "解析到的 release 标签：${INSTALL_TAG}"
    temp_dir="$(mktemp -d)"
    trap 'rm -rf "${temp_dir}"' EXIT

    lancedb_archive="$(download_asset_pair "vldb-lancedb" "${INSTALL_TAG}" "${target}" "${temp_dir}")"
    duckdb_archive="$(download_asset_pair "vldb-duckdb" "${INSTALL_TAG}" "${target}" "${temp_dir}")"

    extract_binary "${lancedb_archive}" "vldb-lancedb" "${temp_dir}"
    extract_binary "${duckdb_archive}" "vldb-duckdb" "${temp_dir}"

    write_lancedb_config "default" "${HOST_BIND}" "${LANCEDB_PORT}"
    write_duckdb_config "default" "${HOST_BIND}" "${DUCKDB_PORT}"
  fi

  install_manager_script
  write_global_config
  ensure_profile_exports

  if [[ "${INSTALL_MODE}" == "full" ]] && confirm_yes_no "Register both services for auto start and auto restart?" "是否注册两个服务为自动启动和自动重启？" "N"; then
    register_default_services
  fi

  if [[ "${INSTALL_MODE}" == "full" ]]; then
    line "Installation completed." "安装完成。"
  else
    line "Controller installation completed." "管理脚本安装完成。"
  fi
  line "Launcher script: ${INSTALL_DIR}/bin/vldg" "管理脚本：${INSTALL_DIR}/bin/vldg"
  line "Re-open your shell or source your profile to use 'vldg' from PATH." "重新打开终端或重新加载 shell 配置后，即可直接使用 'vldg'。"
}

main "$@"
