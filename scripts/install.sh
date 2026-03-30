#!/usr/bin/env bash
set -euo pipefail

SCRIPT_VERSION="0.1.17"
REPO_SLUG="OpenVulcan/vulcan-local-db"
REPO_URL="https://github.com/OpenVulcan/vulcan-local-db"
RAW_BASE_URL="https://raw.githubusercontent.com/${REPO_SLUG}/main/scripts"
GLOBAL_HOME="${HOME}/.vulcan/vldb"
GLOBAL_CONFIG="${GLOBAL_HOME}/config.json"
RUNNER_DIR="${GLOBAL_HOME}/run"
LANG_CODE="en"
INSTALL_DIR=""
INSTALL_TAG=""
LANCEDB_ROOT="${GLOBAL_HOME}/lancedb"
DUCKDB_ROOT="${GLOBAL_HOME}/duckdb"
LATEST_RELEASE_JSON=""
CONTROLLER_SCRIPT_VERSION="${SCRIPT_VERSION}"
INITIALIZED=0
PROMPT_INPUT_FD=""

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

step() {
  line "[Step] $1" "[步骤] $2"
}

initialize_prompt_input() {
  if [[ -n "${PROMPT_INPUT_FD}" ]]; then
    return 0
  fi

  if exec 3<>/dev/tty 2>/dev/null; then
    PROMPT_INPUT_FD="3"
  else
    PROMPT_INPUT_FD="0"
  fi
}

read_prompt_value() {
  local prompt="$1"
  local __resultvar="$2"

  initialize_prompt_input
  if [[ "${PROMPT_INPUT_FD}" == "3" ]]; then
    printf '%s' "${prompt}" >&3
    IFS= read -r -u 3 "${__resultvar}" || return 1
  else
    IFS= read -r -p "${prompt}" "${__resultvar}" || return 1
  fi
}

terminal_line() {
  local message="$1"

  initialize_prompt_input
  if [[ "${PROMPT_INPUT_FD}" == "3" ]]; then
    printf '%s\n' "${message}" >&3
  else
    printf '%s\n' "${message}" >&2
  fi
}

show_banner() {
  printf '%s\n' "===================================="
  printf '%s\n' "       VulcanLocalDB Setup"
  printf '%s\n' "===================================="
  line "The installer now installs only the manager." "安装器现在只负责安装管理器。"
}

prompt_default() {
  local prompt_en="$1"
  local prompt_zh="$2"
  local default_value="$3"
  local answer

  read_prompt_value "$(say "$prompt_en [$default_value]: " "$prompt_zh [$default_value]: ")" answer
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
    read_prompt_value "$(say "$prompt_en [$normalized_default]: " "$prompt_zh [$normalized_default]: ")" answer
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

get_existing_install_dir() {
  local candidate=""
  local config_dir=""

  if [[ -f "${GLOBAL_CONFIG}" ]]; then
    config_dir="$(sed -nE 's/.*"install_dir":[[:space:]]*"([^"]+)".*/\1/p' "${GLOBAL_CONFIG}" | head -n1)"
    if [[ -n "${config_dir}" && -x "${config_dir}/bin/vldb" ]]; then
      printf '%s' "${config_dir}"
      return 0
    fi
  fi

  if [[ -n "${VULCANLOCALDB_HOME:-}" && -x "${VULCANLOCALDB_HOME}/bin/vldb" ]]; then
    printf '%s' "${VULCANLOCALDB_HOME}"
    return 0
  fi

  candidate="$(default_install_dir)"
  if [[ -x "${candidate}/bin/vldb" ]]; then
    printf '%s' "${candidate}"
    return 0
  fi

  return 1
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

default_data_root() {
  case "$1" in
    vldb-lancedb) printf '%s' "${GLOBAL_HOME}/lancedb" ;;
    *) printf '%s' "${GLOBAL_HOME}/duckdb" ;;
  esac
}

default_instance_data_path() {
  local service="$1"
  local instance="$2"
  local lancedb_root="${3:-${LANCEDB_ROOT}}"
  local duckdb_root="${4:-${DUCKDB_ROOT}}"

  if [[ "${service}" == "vldb-lancedb" ]]; then
    printf '%s' "${lancedb_root}/${instance}"
  else
    printf '%s' "${duckdb_root}/${instance}/duckdb.db"
  fi
}

normalize_compare_path() {
  local value="${1%/}"
  printf '%s' "${value}"
}

paths_overlap() {
  local left right
  left="$(normalize_compare_path "$1")"
  right="$(normalize_compare_path "$2")"
  [[ "${left}" == "${right}" || "${left}" == "${right}/"* || "${right}" == "${left}/"* ]]
}

list_install_config_files() {
  local install_root="${1:-${INSTALL_DIR}}"
  local config_dir="${install_root}/config"

  [[ -d "${config_dir}" ]] || return 0
  find "${config_dir}" -maxdepth 1 -type f \( -name "vldb-lancedb-*.json" -o -name "vldb-duckdb-*.json" \) | sort
}

config_db_path() {
  local file="$1"
  sed -nE 's/.*"db_path":[[:space:]]*"([^"]+)".*/\1/p' "${file}" | head -n1
}

validate_data_path() {
  local candidate_path="$1"
  local service="$2"
  local instance="$3"
  local install_root="${4:-${INSTALL_DIR}}"
  local existing_path=""
  local existing_name=""
  local file=""

  if ! is_valid_install_dir "${candidate_path}"; then
    line "Please use a legal absolute data path." "请输入合法的绝对数据路径。"
    return 1
  fi

  if paths_overlap "${install_root}" "${candidate_path}"; then
    line "Database paths must stay outside the installation directory." "数据库路径必须位于安装目录之外。"
    return 1
  fi

  while IFS= read -r file; do
    existing_name="$(basename "${file}" .json)"
    if [[ "${existing_name}" == "${service}-${instance}" ]]; then
      continue
    fi

    existing_path="$(config_db_path "${file}")"
    [[ -n "${existing_path}" ]] || continue

    if paths_overlap "${candidate_path}" "${existing_path}"; then
      line "Data path conflicts with ${existing_name}: ${existing_path}" "数据路径与 ${existing_name} 冲突：${existing_path}"
      return 1
    fi
  done < <(list_install_config_files "${install_root}")

  return 0
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
  read_prompt_value "Select language / 选择语言 [1]: " answer

  case "${answer:-1}" in
    2) LANG_CODE="zh-CN" ;;
    *) LANG_CODE="en" ;;
  esac
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

download_with_progress() {
  local url="$1"
  local output_path="$2"
  local label="$3"

  ensure_command curl
  line "Downloading ${label}" "正在下载 ${label}"
  curl --fail --location --retry 3 --retry-delay 2 --progress-bar --output "${output_path}" "${url}"
  if [[ -f "${output_path}" ]]; then
    local size_bytes
    local size_mb
    size_bytes="$(wc -c < "${output_path}" 2>/dev/null | tr -d '[:space:]')"
    size_mb="$(awk -v bytes="${size_bytes:-0}" 'BEGIN { printf "%.2f", bytes / (1024 * 1024) }')"
    if [[ -n "${size_mb}" ]]; then
      line "Finished downloading ${label} (${size_mb} MB)" "已完成下载 ${label}（${size_mb} MB）"
    else
      line "Finished downloading ${label}" "已完成下载 ${label}"
    fi
  fi
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

  step "Verifying checksum for $(basename "${archive_path}")" "正在校验 $(basename "${archive_path}")"
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

write_global_config() {
  step "Writing global installer config" "正在写入全局安装配置"
  mkdir -p "${GLOBAL_HOME}"
  cat >"${GLOBAL_CONFIG}" <<EOF
{
  "language": "${LANG_CODE}",
  "install_dir": "${INSTALL_DIR}",
  "release_tag": "${INSTALL_TAG}",
  "script_version": "${CONTROLLER_SCRIPT_VERSION}",
  "lancedb_root": "${LANCEDB_ROOT}",
  "duckdb_root": "${DUCKDB_ROOT}",
  "initialized": false
}
EOF
}

invoke_installed_controller_if_present() {
  local existing_install_dir=""
  local controller_path=""

  existing_install_dir="$(get_existing_install_dir || true)"
  [[ -n "${existing_install_dir}" ]] || return 1

  controller_path="${existing_install_dir}/bin/vldb"
  [[ -x "${controller_path}" ]] || return 1

  line "An existing VulcanLocalDB installation was detected at ${existing_install_dir}." "检测到已有 VulcanLocalDB 安装：${existing_install_dir}。"
  line "Launching the local manager script so it can check for updates." "正在启动本地管理脚本，并先检查更新。"
  exec "${controller_path}" --from-installer
}

install_manager_script() {
  local source_dir
  local raw_base
  local installed_script_path

  step "Installing manager script" "正在安装管理脚本"
  source_dir="$(cd "$(dirname "$0")" && pwd)"
  installed_script_path="${INSTALL_DIR}/bin/vldb"
  mkdir -p "${INSTALL_DIR}/bin"

  if [[ -f "${source_dir}/vldb" ]]; then
    install -m 755 "${source_dir}/vldb" "${installed_script_path}"
  else
    raw_base="${RAW_BASE_URL}"
    download_with_progress "${raw_base}/vldb" "${installed_script_path}" "vldb"
    chmod 755 "${installed_script_path}"
  fi

  CONTROLLER_SCRIPT_VERSION="$(extract_script_version_from_file "${installed_script_path}" || true)"
  CONTROLLER_SCRIPT_VERSION="${CONTROLLER_SCRIPT_VERSION:-${SCRIPT_VERSION}}"
}

global_launcher_path() {
  case "$(uname -s)" in
    Linux|Darwin)
      if [[ "$(id -u)" -eq 0 ]]; then
        printf '%s' "/usr/local/bin/vldb"
      else
        printf '%s' "${HOME}/.local/bin/vldb"
      fi
      ;;
    *)
      printf '%s' "${INSTALL_DIR}/bin/vldb"
      ;;
  esac
}

install_global_launcher() {
  local launcher_path
  local launcher_dir

  launcher_path="$(global_launcher_path)"
  launcher_dir="$(dirname "${launcher_path}")"

  mkdir -p "${launcher_dir}"
  cat >"${launcher_path}" <<EOF
#!/usr/bin/env bash
exec "${INSTALL_DIR}/bin/vldb" "\$@"
EOF
  chmod 755 "${launcher_path}"
}

ensure_profile_exports() {
  local profile_file
  local marker_begin="# VulcanLocalDB begin"
  local marker_end="# VulcanLocalDB end"

  step "Updating shell profile and PATH" "正在更新 shell 配置和 PATH"
  profile_file="$(detect_profile_file)"
  mkdir -p "$(dirname "${profile_file}")"
  touch "${profile_file}"

  if ! grep -Fq "${marker_begin}" "${profile_file}"; then
    cat >>"${profile_file}" <<EOF
${marker_begin}
export VULCANLOCALDB_HOME="${INSTALL_DIR}"
export VULCANLOCALDB_BIN="${INSTALL_DIR}/bin"
export PATH="${INSTALL_DIR}/bin:\$PATH"
${marker_end}
EOF
  fi

  export VULCANLOCALDB_HOME="${INSTALL_DIR}"
  export VULCANLOCALDB_BIN="${INSTALL_DIR}/bin"
  case ":${PATH}:" in
    *":${INSTALL_DIR}/bin:"*) ;;
    *) export PATH="${INSTALL_DIR}/bin:${PATH}" ;;
  esac
}

write_runner_script() {
  local service="$1"
  local instance="$2"
  local config_path="${INSTALL_DIR}/config/${service}-${instance}.json"
  local runner_path="${RUNNER_DIR}/${service}-${instance}.sh"

  step "Writing service runner for ${service} (${instance})" "正在为 ${service}（${instance}）生成服务启动脚本"
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

  step "Registering systemd service for ${service} (${instance})" "正在为 ${service}（${instance}）注册 systemd 服务"
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

  step "Registering launchd service for ${service} (${instance})" "正在为 ${service}（${instance}）注册 launchd 服务"
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

main() {
  show_banner
  choose_language
  show_update_notice

  if invoke_installed_controller_if_present; then
    return 0
  fi

  choose_install_dir
  install_manager_script
  install_global_launcher
  write_global_config
  ensure_profile_exports
  line "Manager script installation completed." "管理脚本安装完成。"
  line "Manager command: ${INSTALL_DIR}/bin/vldb" "管理命令：${INSTALL_DIR}/bin/vldb"
  line "Global command: $(global_launcher_path)" "全局命令：$(global_launcher_path)"
  line "Launching the manager to continue installation." "正在启动管理器继续完成安装。"
  exec "${INSTALL_DIR}/bin/vldb" --from-installer
}

main "$@"
