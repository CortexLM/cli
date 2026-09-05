#!/bin/sh
# Install Cortex CLI from https://software.cortex.foundation
#
# Usage:
#   curl -fsSL https://software.cortex.foundation/install.sh | sh
#
# Optional environment:
#   CORTEX_VERSION       Pin a version (e.g. 0.1.2). Default: latest on the channel.
#   CORTEX_CHANNEL       stable (default), beta, or nightly
#   CORTEX_INSTALL_DIR   Prefix (default: $HOME/.local). Binary goes in PREFIX/bin.
#   CORTEX_SOFTWARE_URL  Override the distribution host (testing only).
#
# Downloads the matching archive, verifies SHA-256, then installs the binary.
# Checksum verification is required; the script will not install an unverified file.

set -eu

SOFTWARE_URL="${CORTEX_SOFTWARE_URL:-https://software.cortex.foundation}"
CHANNEL="${CORTEX_CHANNEL:-stable}"
PREFIX="${CORTEX_INSTALL_DIR:-${HOME}/.local}"
BIN_DIR="${PREFIX}/bin"
TMPDIR="${TMPDIR:-/tmp}"
WORKDIR=""

cleanup() {
    if [ -n "$WORKDIR" ] && [ -d "$WORKDIR" ]; then
        rm -rf "$WORKDIR"
    fi
}
trap cleanup EXIT INT HUP

die() {
    echo "install.sh: $*" >&2
    exit 1
}

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

py() {
    if command -v python3 >/dev/null 2>&1; then
        python3 "$@"
    elif command -v python >/dev/null 2>&1; then
        python "$@"
    else
        die "need python3 (or python) to parse release JSON"
    fi
}

detect_platform() {
    os=$(uname -s)
    arch=$(uname -m)
    case "$os" in
        Linux) os_key=linux ;;
        Darwin) os_key=darwin ;;
        *) die "unsupported OS: $os (supported: Linux, macOS)" ;;
    esac
    case "$arch" in
        x86_64|amd64) arch_key=x86_64 ;;
        aarch64|arm64) arch_key=aarch64 ;;
        *) die "unsupported architecture: $arch (supported: x86_64, aarch64)" ;;
    esac
    echo "${os_key}-${arch_key}"
}

download() {
    url="$1"
    dest="$2"
    curl -fsSL --retry 3 --retry-delay 1 -o "$dest" "$url" || die "download failed: $url"
}

try_download() {
    url="$1"
    dest="$2"
    curl -fsSL --retry 3 --retry-delay 1 -o "$dest" "$url"
}

verify_sha256() {
    file="$1"
    expected="$2"
    expected=$(printf '%s' "$expected" | tr 'A-F' 'a-f' | tr -d ' \t\r\n')
    if command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "$file" | awk '{print $1}')
    elif command -v shasum >/dev/null 2>&1; then
        actual=$(shasum -a 256 "$file" | awk '{print $1}')
    else
        die "need sha256sum or shasum to verify the download"
    fi
    actual=$(printf '%s' "$actual" | tr 'A-F' 'a-f')
    if [ "$actual" != "$expected" ]; then
        die "SHA-256 mismatch for $(basename "$file"): expected $expected, got $actual"
    fi
}

need_cmd curl
need_cmd tar
need_cmd uname
need_cmd awk
need_cmd mkdir
need_cmd chmod
need_cmd find
need_cmd cp
need_cmd ln

case "$CHANNEL" in
    stable|beta|nightly) ;;
    *) die "invalid CORTEX_CHANNEL='$CHANNEL' (use stable, beta, or nightly)" ;;
esac

PLATFORM=$(detect_platform)
WORKDIR=$(mktemp -d "${TMPDIR}/cortex-install.XXXXXX")

echo "Cortex CLI installer"
echo "  host:     $SOFTWARE_URL"
echo "  platform: $PLATFORM"
echo "  prefix:   $PREFIX"

RELEASE_JSON="${WORKDIR}/release.json"
VERSION="${CORTEX_VERSION:-}"
if [ -n "$VERSION" ]; then
    VERSION=$(printf '%s' "$VERSION" | sed 's/^v//')
fi

if [ -n "$VERSION" ]; then
    if try_download "${SOFTWARE_URL}/releases/${VERSION}.json" "$RELEASE_JSON" \
        || try_download "${SOFTWARE_URL}/v1/releases/${VERSION}.json" "$RELEASE_JSON"; then
        :
    else
        die "could not fetch release metadata for ${VERSION} from ${SOFTWARE_URL}"
    fi
else
    MANIFEST="${WORKDIR}/manifest.json"
    if try_download "${SOFTWARE_URL}/releases/manifest.json" "$MANIFEST" \
        || try_download "${SOFTWARE_URL}/v1/releases/manifest.json" "$MANIFEST"; then
        :
    else
        die "could not fetch ${SOFTWARE_URL}/releases/manifest.json"
    fi
    py -c "
import json, sys
channel = sys.argv[2]
data = json.load(open(sys.argv[1]))
info = data.get(channel)
if not info:
    sys.stderr.write('install.sh: no %s release in manifest\n' % channel)
    sys.exit(1)
json.dump(info, open(sys.argv[3], 'w'))
print(info['version'])
" "$MANIFEST" "$CHANNEL" "$RELEASE_JSON" > "${WORKDIR}/version.txt"
    VERSION=$(cat "${WORKDIR}/version.txt")
fi

ASSET_URL=$(py -c "
import json, sys
data = json.load(open(sys.argv[1]))
assets = data.get('assets') or {}
asset = assets.get(sys.argv[2])
if not asset:
    sys.stderr.write('install.sh: no asset for platform %s\n' % sys.argv[2])
    sys.exit(1)
print(asset['url'])
print(asset['sha256'])
" "$RELEASE_JSON" "$PLATFORM")

URL=$(printf '%s\n' "$ASSET_URL" | sed -n '1p')
SHA=$(printf '%s\n' "$ASSET_URL" | sed -n '2p')
[ -n "$URL" ] && [ -n "$SHA" ] || die "release JSON missing url/sha256 for ${PLATFORM}"

case "$URL" in
    *.zip) ARCHIVE_NAME=cortex.zip ;;
    *) ARCHIVE_NAME=cortex.tar.gz ;;
esac

echo "  version:  $VERSION"
echo "  download: $URL"

ARCHIVE="${WORKDIR}/${ARCHIVE_NAME}"
download "$URL" "$ARCHIVE"
verify_sha256 "$ARCHIVE" "$SHA"
echo "  checksum: ok"

EXTRACT="${WORKDIR}/extract"
mkdir -p "$EXTRACT"
case "$ARCHIVE_NAME" in
    *.zip)
        need_cmd unzip
        unzip -q "$ARCHIVE" -d "$EXTRACT"
        ;;
    *)
        tar -xzf "$ARCHIVE" -C "$EXTRACT"
        ;;
esac

BINARY=""
for candidate in Cortex cortex; do
    found=$(find "$EXTRACT" -type f -name "$candidate" | head -n 1)
    if [ -n "$found" ]; then
        BINARY="$found"
        break
    fi
done
[ -n "$BINARY" ] || die "archive did not contain a Cortex binary"

mkdir -p "$BIN_DIR"
install_path="${BIN_DIR}/Cortex"
cp "$BINARY" "$install_path"
chmod 755 "$install_path"
ln -sf Cortex "${BIN_DIR}/cortex"
ln -sf Cortex "${BIN_DIR}/agent"

echo "Installed Cortex CLI v${VERSION} to ${install_path}"

case ":${PATH}:" in
    *:"${BIN_DIR}":*) ;;
    *)
        echo ""
        echo "Add ${BIN_DIR} to PATH, for example:"
        echo "  export PATH=\"${BIN_DIR}:\$PATH\""
        echo "Then restart your shell and run: cortex --version"
        ;;
esac
