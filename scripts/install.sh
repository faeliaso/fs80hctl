#!/usr/bin/env bash
# Install the latest fs80hctl Linux release from GitHub.

set -euo pipefail

readonly REPOSITORY="faeliaso/fs80hctl"
readonly RELEASE_API_URL="https://api.github.com/repos/${REPOSITORY}/releases/latest"
readonly RELEASE_DOWNLOAD_URL="https://github.com/${REPOSITORY}/releases/download"
readonly UDEV_RULE="99-futronic-fs80h.rules"

temp_dir=""

info() {
  printf '[INFO] %s\n' "$*"
}

success() {
  printf '[OK] %s\n' "$*"
}

error() {
  printf '[ERROR] %s\n' "$*" >&2
}

cleanup() {
  if [[ -n "${temp_dir}" && -d "${temp_dir}" ]]; then
    rm -rf -- "${temp_dir}"
  fi
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    error "Required command not found: $1"
    exit 1
  fi
}

detect_target() {
  case "$(uname -m)" in
    x86_64|amd64)
      printf '%s\n' "x86_64-unknown-linux-gnu"
      ;;
    aarch64|arm64)
      printf '%s\n' "aarch64-unknown-linux-gnu"
      ;;
    *)
      error "Unsupported architecture: $(uname -m). Supported: x86_64, aarch64, arm64."
      exit 1
      ;;
  esac
}

latest_tag() {
  local release_json tag

  release_json="$(curl --fail --location --silent --show-error --retry 3 "${RELEASE_API_URL}")"
  tag="$(sed -n 's/^[[:space:]]*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' <<<"${release_json}" | head -n 1)"

  if [[ -z "${tag}" ]]; then
    error "Unable to determine the latest release tag from GitHub."
    exit 1
  fi

  printf '%s\n' "${tag}"
}

main() {
  require_command curl
  require_command tar
  require_command sha256sum
  require_command udevadm

  if [[ "${EUID}" -ne 0 ]]; then
    error "Root privileges are required to install fs80hctl in /usr/local and /etc."
    printf 'Re-run this installer with: sudo %q\n' "$0" >&2
    exit 1
  fi

  local target tag version archive checksum archive_url checksum_url
  target="$(detect_target)"
  info "Detected target: ${target}"

  info "Looking up the latest fs80hctl release..."
  tag="$(latest_tag)"
  version="${tag#v}"
  archive="fs80hctl-v${version}-${target}.tar.gz"
  checksum="${archive}.sha256"
  archive_url="${RELEASE_DOWNLOAD_URL}/${tag}/${archive}"
  checksum_url="${RELEASE_DOWNLOAD_URL}/${tag}/${checksum}"

  temp_dir="$(mktemp -d)"
  trap cleanup EXIT

  info "Downloading ${archive} from ${tag}..."
  curl --fail --location --silent --show-error --retry 3 --output "${temp_dir}/${archive}" "${archive_url}"
  curl --fail --location --silent --show-error --retry 3 --output "${temp_dir}/${checksum}" "${checksum_url}"

  info "Verifying release checksum..."
  (
    cd "${temp_dir}"
    sha256sum --check "${checksum}"
  )

  tar -xzf "${temp_dir}/${archive}" -C "${temp_dir}"

  local release_root="${temp_dir}"
  [[ -f "${release_root}/fs80hctl" ]] || {
    error "Release archive does not contain fs80hctl at its root."
    exit 1
  }
  [[ -f "${release_root}/udev/${UDEV_RULE}" ]] || {
    error "Release archive does not contain udev/${UDEV_RULE}."
    exit 1
  }

  info "Installing fs80hctl..."
  install -Dm 0755 "${release_root}/fs80hctl" /usr/local/bin/fs80hctl
  install -Dm 0644 "${release_root}/udev/${UDEV_RULE}" "/etc/udev/rules.d/${UDEV_RULE}"

  if [[ -f "${release_root}/man/fs80hctl.1" ]]; then
    install -Dm 0644 "${release_root}/man/fs80hctl.1" /usr/local/share/man/man1/fs80hctl.1
  fi

  info "Reloading udev rules..."
  udevadm control --reload-rules
  udevadm trigger

  if [[ -d /run/systemd/system ]] && command -v systemctl >/dev/null 2>&1; then
    [[ -f "${release_root}/systemd/fs80h.service" ]] || {
      error "Release archive does not contain systemd/fs80h.service."
      exit 1
    }
    [[ -f "${release_root}/systemd/fs80h.socket" ]] || {
      error "Release archive does not contain systemd/fs80h.socket."
      exit 1
    }

    info "Installing systemd units..."
    install -Dm 0644 "${release_root}/systemd/fs80h.service" /etc/systemd/system/fs80h.service
    install -Dm 0644 "${release_root}/systemd/fs80h.socket" /etc/systemd/system/fs80h.socket
    systemctl daemon-reload
    success "Systemd units installed. Enable the socket with: systemctl enable --now fs80h.socket"
  else
    info "systemd is not active; skipping systemd unit installation."
  fi

  success "fs80hctl ${version} installed successfully."
  success "Verify with: fs80hctl --version"
}

main "$@"
