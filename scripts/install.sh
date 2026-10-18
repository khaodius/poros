#!/usr/bin/env bash
# Installs Poros on x86-64 Linux from its GitHub releases: the latest release, or the version
# given as an argument. Debian and Ubuntu get the .deb through apt; Fedora, RHEL and openSUSE get
# the .rpm through dnf, zypper or yum; other distributions, or any with --appimage, get the
# AppImage in ~/.local/bin with a menu entry, which needs no root. Every download is checked
# against the release's SHA256SUMS.txt before anything is installed.
#
#   curl -fsSL https://raw.githubusercontent.com/khaodius/poros/main/scripts/install.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/khaodius/poros/main/scripts/install.sh | bash -s -- 0.4.9 --appimage

set -euo pipefail

readonly releases="https://github.com/khaodius/poros/releases"

workdir=""

usage() {
  cat << 'EOF'
Usage: install.sh [VERSION] [--appimage]

Installs the latest Poros release, or VERSION (such as 0.4.9).

  --appimage  install the AppImage in ~/.local/bin for this user only, instead of the
              .deb or .rpm package for the whole system
EOF
}

say() {
  printf '%s\n' "$*"
}

fail() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

cleanup() {
  if [ -n "$workdir" ]; then
    rm -rf "$workdir"
  fi
}

download() {
  local url="$1" destination="$2" output=(--silent --show-error)
  if [ "${3:-}" = progress ] && [ -t 2 ]; then
    output=(--progress-bar)
  fi
  curl --fail --location --proto '=https' --tlsv1.2 --retry 3 "${output[@]}" --output "$destination" "$url"
}

sha256_of() {
  if command -v sha256sum > /dev/null; then
    sha256sum "$1" | cut -d ' ' -f 1
  else
    shasum -a 256 "$1" | cut -d ' ' -f 1
  fi
}

run_as_root() {
  if [ "$(id -u)" -eq 0 ]; then
    "$@"
  else
    sudo "$@"
  fi
}

install_deb() {
  # apt reads local packages as its unprivileged _apt user.
  chmod 755 "$workdir"
  chmod 644 "$1"
  say "Installing with apt"
  run_as_root apt-get update -qq || say "warning: apt-get update failed, installing with the package lists as they are"
  run_as_root env DEBIAN_FRONTEND=noninteractive apt-get install -y -q --allow-downgrades "$1"
}

install_rpm() {
  if command -v dnf > /dev/null; then
    say "Installing with dnf"
    run_as_root dnf install -y "$1"
  elif command -v zypper > /dev/null; then
    say "Installing with zypper"
    run_as_root zypper --non-interactive install --allow-unsigned-rpm "$1"
  else
    say "Installing with yum"
    run_as_root yum install -y "$1"
  fi
}

install_appimage() {
  local appimage="$1"
  local bin_directory="${XDG_BIN_HOME:-$HOME/.local/bin}"
  local data_directory="${XDG_DATA_HOME:-$HOME/.local/share}"
  local target="$bin_directory/poros"

  say "Installing in $target"
  chmod 755 "$appimage"
  # The menu entry and icons come out of this AppImage, so they always match it.
  (
    cd "$workdir"
    "$appimage" --appimage-extract 'usr/share/applications/*' > /dev/null
    "$appimage" --appimage-extract 'usr/share/icons/*' > /dev/null
  ) || fail "could not unpack the menu entry from the AppImage"

  mkdir -p "$bin_directory" "$data_directory/applications" "$data_directory/icons"
  # A rename replaces the file even while Poros runs from it.
  cp "$appimage" "$target.partial"
  mv -f "$target.partial" "$target"

  cp -R "$workdir/squashfs-root/usr/share/icons/hicolor" "$data_directory/icons/"
  local entry
  for entry in "$workdir"/squashfs-root/usr/share/applications/*.desktop; do
    sed "s|^Exec=.*|Exec=\"$target\"|" "$entry" > "$data_directory/applications/$(basename "$entry")"
  done
  if command -v update-desktop-database > /dev/null; then
    update-desktop-database "$data_directory/applications" > /dev/null 2>&1 || true
  fi

  if ! command -v fusermount3 > /dev/null && ! command -v fusermount > /dev/null; then
    say "note: AppImages need FUSE; install your distribution's fuse3 package if Poros does not start"
  fi
  case ":$PATH:" in
    *":$bin_directory:"*) ;;
    *) say "note: $bin_directory is not on your PATH, so start Poros from the applications menu or as $target" ;;
  esac
}

main() {
  local requested_version="" format="" argument
  for argument in "$@"; do
    case "$argument" in
      --appimage) format=appimage ;;
      -h | --help)
        usage
        return
        ;;
      v[0-9]* | [0-9]*) requested_version="${argument#v}" ;;
      *)
        usage >&2
        fail "unknown argument: $argument"
        ;;
    esac
  done

  [ "$(uname -s)" = Linux ] || fail "this script installs Poros on Linux; on Windows use install.ps1 or install.cmd"
  local machine
  machine=$(uname -m)
  [ "$machine" = x86_64 ] || fail "Poros has Linux builds for x86-64 only, and this machine is $machine"
  command -v curl > /dev/null || fail "curl is needed to download Poros"
  command -v sha256sum > /dev/null || command -v shasum > /dev/null ||
    fail "sha256sum or shasum is needed to check the download"

  if [ -z "$format" ]; then
    if command -v apt-get > /dev/null && command -v dpkg > /dev/null; then
      format=deb
    elif command -v dnf > /dev/null || command -v zypper > /dev/null || command -v yum > /dev/null; then
      format=rpm
    else
      format=appimage
    fi
  fi
  if [ "$format" != appimage ] && [ "$(id -u)" -ne 0 ] && ! command -v sudo > /dev/null; then
    fail "installing the .$format package needs root: run this as root, or pass --appimage to install for your user only"
  fi

  # Release file names carry the version between this prefix and suffix.
  local prefix suffix
  case "$format" in
    deb) prefix=Poros_ suffix=_amd64.deb ;;
    rpm) prefix=Poros- suffix=-1.x86_64.rpm ;;
    appimage) prefix=Poros_ suffix=_amd64.AppImage ;;
  esac

  workdir=$(mktemp -d)
  trap cleanup EXIT

  local checksums_url
  if [ -n "$requested_version" ]; then
    checksums_url="$releases/download/v$requested_version/SHA256SUMS.txt"
  else
    checksums_url="$releases/latest/download/SHA256SUMS.txt"
  fi
  download "$checksums_url" "$workdir/SHA256SUMS.txt" ||
    fail "could not download $checksums_url${requested_version:+; there may be no $requested_version release}"

  local expected_hash="" file_name="" listed_hash listed_name
  while read -r listed_hash listed_name; do
    listed_name="${listed_name#\*}"
    case "$listed_name" in
      "$prefix"*"$suffix")
        expected_hash="$listed_hash"
        file_name="$listed_name"
        ;;
    esac
  done < "$workdir/SHA256SUMS.txt"
  [ -n "$file_name" ] || fail "the release has no $prefix*$suffix file"
  local version="${file_name#"$prefix"}"
  version="${version%"$suffix"}"

  say "Downloading Poros $version ($file_name)"
  local package="$workdir/$file_name"
  download "$releases/download/v$version/$file_name" "$package" progress ||
    fail "could not download $file_name"
  [ "$(sha256_of "$package")" = "$expected_hash" ] ||
    fail "$file_name does not match its SHA-256 checksum in SHA256SUMS.txt, so nothing was installed"

  case "$format" in
    deb) install_deb "$package" ;;
    rpm) install_rpm "$package" ;;
    appimage) install_appimage "$package" ;;
  esac

  if command -v poros > /dev/null; then
    say "Poros $version is installed. Open it from your applications menu, or run poros."
  else
    say "Poros $version is installed. Open it from your applications menu."
  fi
}

# Nothing runs until bash has read this whole file, so a download cut short installs nothing and
# no command the script starts can read the rest of the script from the pipe.
main "$@"
