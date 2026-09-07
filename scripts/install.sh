#!/bin/sh
# Install Cortex CLI from the configured distribution origin.
# Optional: CORTEX_VERSION, CORTEX_CHANNEL, CORTEX_INSTALL_DIR (prefix).
# CORTEX_SOFTWARE_URL may select an explicit test origin; HTTP is loopback-only.
# Checksums detect corruption, not publisher identity. No signing is implied.
set -eu
umask 077

SOFTWARE_URL="${CORTEX_SOFTWARE_URL:-https://software.cortex.foundation}"
CHANNEL="${CORTEX_CHANNEL:-stable}"
PREFIX="${CORTEX_INSTALL_DIR:-${HOME}/.local}"
WORKDIR=""
cleanup() {
    if [ -n "$WORKDIR" ]; then
        python3 -c 'import shutil,sys; shutil.rmtree(sys.argv[1])' "$WORKDIR"
    fi
}
trap cleanup 0
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
die() { echo "install.sh: $*" >&2; exit 1; }
for cmd in uname mktemp python3; do
    command -v "$cmd" >/dev/null 2>&1 || die "missing required command: $cmd"
done
case "$CHANNEL" in stable|beta|nightly) ;; *) die "invalid release channel" ;; esac

os=$(uname -s)
arch=$(uname -m)
case "$arch" in
    x86_64|amd64) arch=x86_64 ;;
    aarch64|arm64) arch=aarch64 ;;
    *) die "unsupported architecture: $arch" ;;
esac
case "$os" in
    Darwin) PLATFORM="darwin-$arch" ;;
    Linux)
        if getconf GNU_LIBC_VERSION >/dev/null 2>&1; then
            PLATFORM="linux-$arch"
        elif { ldd --version 2>&1 || true; } | grep -qi musl; then
            PLATFORM="linux-$arch-musl"
        elif [ -f "/lib/ld-musl-$arch.so.1" ]; then
            PLATFORM="linux-$arch-musl"
        else
            die "cannot determine Linux libc; use a documented release archive"
        fi ;;
    *) die "unsupported OS: $os (supported: Linux, macOS)" ;;
esac

# Reject credentials, insecure remote origins, and URL suffixes before any request.
SOFTWARE_URL=$(python3 - "$SOFTWARE_URL" <<'PYURL'
import sys
from urllib.parse import urlsplit
url = sys.argv[1].rstrip('/')
u = urlsplit(url)
if (u.scheme not in ('https', 'http') or not u.hostname or u.username or u.password
        or u.query or u.fragment or u.path not in ('', '/')
        or (u.scheme == 'http' and u.hostname not in ('127.0.0.1', 'localhost', '::1'))):
    sys.exit('install.sh: distribution URL must be an HTTPS origin (HTTP only for loopback tests)')
print(url)
PYURL
)
VERSION=$(python3 - "${CORTEX_VERSION:-}" <<'PYVERSION'
import re, sys
v = sys.argv[1]
v = v[1:] if v.startswith('v') else v
if v and not re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?', v):
    sys.exit('install.sh: invalid version')
print(v)
PYVERSION
)
WORKDIR=$(mktemp -d "${TMPDIR:-/tmp}/cortex-install.XXXXXX")
# Do not follow redirects to an unconfigured origin. Bound headers/body/time.
download() {
    python3 - "$1" "$2" "$3" <<'PYDOWNLOAD'
import pathlib, sys, time
from urllib.request import HTTPRedirectHandler, Request, build_opener
class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None
url, target, limit = sys.argv[1], pathlib.Path(sys.argv[2]), int(sys.argv[3])
try:
    deadline = time.monotonic() + 120
    with build_opener(NoRedirect()).open(Request(url), timeout=10) as response, target.open('wb') as out:
        if response.status != 200 or int(response.headers.get('Content-Length', 0)) > limit:
            raise ValueError('unexpected status or oversized download')
        size = 0
        while True:
            chunk = response.read(min(65536, limit - size + 1))
            size += len(chunk)
            if size > limit or time.monotonic() > deadline:
                raise ValueError('download exceeds size/time limit')
            if not chunk:
                break
            out.write(chunk)
except Exception:
    sys.exit('install.sh: download failed or exceeded size/time limit')
PYDOWNLOAD
}
RELEASE_JSON="$WORKDIR/release.json"
if [ -n "$VERSION" ]; then
    download "$SOFTWARE_URL/releases/$VERSION.json" "$RELEASE_JSON" 2097152 \
        || download "$SOFTWARE_URL/v1/releases/$VERSION.json" "$RELEASE_JSON" 2097152 \
        || die "release metadata unavailable"
else
    download "$SOFTWARE_URL/releases/manifest.json" "$RELEASE_JSON" 2097152 \
        || download "$SOFTWARE_URL/v1/releases/manifest.json" "$RELEASE_JSON" 2097152 \
        || die "release manifest unavailable"
fi

python3 - "$RELEASE_JSON" "$PLATFORM" "$VERSION" "$CHANNEL" "$SOFTWARE_URL" "$WORKDIR" <<'PYMETA'
import json, pathlib, re, sys
source, platform, pin, channel, origin, work = sys.argv[1:]
data = json.loads(pathlib.Path(source).read_bytes())
release = data if pin else data.get(channel)
if not isinstance(release, dict):
    sys.exit('install.sh: selected release is missing')
version = release.get('version', '')
if not re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?', version):
    sys.exit('install.sh: invalid release version')
if (pin and pin != version) or release.get('channel') != channel:
    sys.exit('install.sh: release version/channel mismatch')
asset = (release.get('assets') or {}).get(platform)
if not isinstance(asset, dict):
    sys.exit('install.sh: no asset for platform ' + platform)
url, sha, size = asset.get('url'), asset.get('sha256', ''), asset.get('size')
expected_url = f'{origin}/v1/assets/{platform}/{version}/cortex.tar.gz'
if url != expected_url or not re.fullmatch('[a-fA-F0-9]{64}', sha):
    sys.exit('install.sh: invalid asset URL or SHA-256')
if type(size) is not int or not 0 < size <= 512 * 1024 * 1024:
    sys.exit('install.sh: invalid or oversized asset')
for key, value in [('url',url),('sha',sha.lower()),('size',size),('version',version)]:
    (pathlib.Path(work) / key).write_text(str(value))
PYMETA
URL=$(cat "$WORKDIR/url")
SIZE=$(cat "$WORKDIR/size")
echo "Cortex CLI installer: $PLATFORM"
download "$URL" "$WORKDIR/cortex.tar.gz" "$SIZE" || die "archive download failed"

# Only extract the release's single regular binary, never archive paths or links.
# Stage on the destination filesystem. Preserve the previous binary on failure.
python3 - "$WORKDIR" "$PREFIX/bin" <<'PYINSTALL'
import gzip, hashlib, os, pathlib, shutil, subprocess, sys, tarfile, tempfile
work, bindir = map(pathlib.Path, sys.argv[1:])
archive = work / 'cortex.tar.gz'
version = (work / 'version').read_text()
if archive.stat().st_size != int((work / 'size').read_text()):
    sys.exit('install.sh: archive size mismatch')
with archive.open('rb') as stream:
    digest = hashlib.sha256()
    for chunk in iter(lambda: stream.read(1024 * 1024), b''):
        digest.update(chunk)
    actual = digest.hexdigest()
if actual != (work / 'sha').read_text():
    sys.exit('install.sh: SHA-256 mismatch')
bindir.mkdir(parents=True, exist_ok=True)
dest, backup = bindir / 'Cortex', bindir / 'Cortex.old'
for path in (dest, backup):
    if path.is_symlink() or (path.exists() and not path.is_file()):
        sys.exit('install.sh: refusing non-regular installation target')
for name in ('cortex', 'agent'):
    path = bindir / name
    if os.path.lexists(path) and not (path.is_symlink() and os.readlink(path) == 'Cortex'):
        sys.exit('install.sh: refusing to overwrite existing command: ' + name)

def verify_binary(path):
    result = subprocess.run([str(path.resolve()), '--version'], capture_output=True, timeout=15)
    words = result.stdout.decode('utf-8', errors='replace').strip().split()
    if result.returncode or not words or words[-1] != version:
        raise ValueError('installed binary version check failed')

with tempfile.TemporaryDirectory(prefix='.cortex-install-', dir=bindir) as temp:
    stage = pathlib.Path(temp) / 'Cortex'
    class BoundedTarStream:
        def __init__(self, stream):
            self.stream = stream
            self.remaining = 512 * 1024 * 1024 + 1024 * 1024
        def read(self, size):
            data = self.stream.read(min(size, self.remaining + 1))
            self.remaining -= len(data)
            if self.remaining < 0:
                raise ValueError('archive exceeds decompressed size limit')
            return data
    with gzip.open(archive, 'rb') as compressed, tarfile.open(fileobj=BoundedTarStream(compressed), mode='r|') as package:
        count = 0
        for member in package:
            count += 1
            if count != 1 or member.name not in ('Cortex', './Cortex') or not member.isfile():
                raise ValueError('archive must contain only the regular Cortex binary')
            if not 0 < member.size <= 512 * 1024 * 1024:
                raise ValueError('invalid extracted binary size')
            with package.extractfile(member) as source, stage.open('xb') as target:
                shutil.copyfileobj(source, target, 1024 * 1024)
        if count != 1:
            raise ValueError('archive did not contain Cortex')
    stage.chmod(0o755)
    verify_binary(stage)
    had_previous = dest.exists()
    if had_previous:
        # Copy into private staging before atomically replacing the recovery file.
        saved = pathlib.Path(temp) / 'previous'
        shutil.copy2(dest, saved)
        os.replace(saved, backup)
    replaced = False
    aliases = []
    try:
        os.replace(stage, dest)
        replaced = True
        verify_binary(dest)
        for name in ('cortex', 'agent'):
            path = bindir / name
            if not os.path.lexists(path):
                path.symlink_to('Cortex')
                aliases.append(path)
    except BaseException:
        for path in aliases:
            path.unlink()
        if replaced:
            if had_previous:
                os.replace(backup, dest)
            else:
                dest.unlink()
        raise
print(f'Installed Cortex CLI v{version} to {dest}')
if backup.exists():
    print(f'Previous binary retained at {backup}')
PYINSTALL
case ":${PATH}:" in
    *:"${PREFIX}/bin":*) ;;
    *) echo "Add ${PREFIX}/bin to PATH, then run: cortex --version" ;;
esac
