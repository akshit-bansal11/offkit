#!/usr/bin/env sh
# Downloads the PDFium library the PDF tools bind at run time (bblanchon/pdfium-binaries,
# chromium/8100), checks its SHA-256 and unpacks it into desktop/vendor/<os>/pdfium/, the folder
# debug builds look in (src/engine/pdf.rs). The archive's LICENSE and licenses/ come along: they
# must ship with the library.
#
#   sh desktop/scripts/fetch-pdfium.sh               # this machine's platform
#   sh desktop/scripts/fetch-pdfium.sh mac-arm64     # another one: win-x64, mac-arm64, mac-x64, linux-x64
#
# Needs curl, tar and sha256sum (or shasum). On Windows, run it from Git Bash.
set -eu

RELEASE="chromium/8100"
# GitHub's published digest per asset. win-x64 was also downloaded and hashed locally; it matched.
hash_of() {
    case "$1" in
        win-x64) echo 51621bcc1d9be9ea448f4b84b0516862b928e3968d3fca899af1a6d6e2848d6d ;;
        mac-arm64) echo 7ea647c7a7a9c1169be3f947000bc071ece2fade2cd6bd7e401e3675c6560cce ;;
        mac-x64) echo d513a0530a0f0c1ca9929ded06660e27edb9a108a7d18d9df55a5fb060b091c8 ;;
        linux-x64) echo 1c1894a12d4d96ee01400c3874444126254180c5c7d2ab628576b67f8120320a ;;
        *) return 1 ;;
    esac
}

if [ $# -gt 0 ]; then
    target="$1"
else
    case "$(uname -s)-$(uname -m)" in
        MINGW*-x86_64 | MSYS*-x86_64 | CYGWIN*-x86_64) target=win-x64 ;;
        Darwin-arm64) target=mac-arm64 ;;
        Darwin-x86_64) target=mac-x64 ;;
        Linux-x86_64) target=linux-x64 ;;
        *) echo "No PDFium build is listed for $(uname -s) $(uname -m)." >&2; exit 1 ;;
    esac
fi
expected=$(hash_of "$target") || { echo "Unknown platform “$target”: use win-x64, mac-arm64, mac-x64 or linux-x64." >&2; exit 1; }

# Rust's std::env::consts::OS names, which the locator uses.
case "$target" in
    win-*) os=windows; library=pdfium.dll ;;
    mac-*) os=macos; library=libpdfium.dylib ;;
    linux-*) os=linux; library=libpdfium.so ;;
esac

if command -v sha256sum >/dev/null 2>&1; then
    sha256() { sha256sum "$1" | cut -d' ' -f1; }
else
    sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
fi

desktop=$(cd "$(dirname "$0")/.." && pwd)
dest="$desktop/vendor/$os/pdfium"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

archive="$work/pdfium-$target.tgz"
url="https://github.com/bblanchon/pdfium-binaries/releases/download/$(echo "$RELEASE" | sed 's|/|%2F|')/pdfium-$target.tgz"
echo "Downloading $url"
curl --fail --location --silent --show-error --user-agent offkit-build-check --output "$archive" "$url"

actual=$(sha256 "$archive")
if [ "$actual" != "$expected" ]; then
    echo "Refusing pdfium-$target.tgz: its SHA-256 is $actual, expected $expected." >&2
    exit 1
fi

mkdir "$work/x"
tar -xzf "$archive" -C "$work/x"
found=""
for dir in bin lib; do
    [ -f "$work/x/$dir/$library" ] && found="$work/x/$dir/$library"
done
[ -n "$found" ] || { echo "The archive has no $library in bin/ or lib/." >&2; exit 1; }

rm -rf "$dest"
mkdir -p "$dest"
cp "$found" "$dest/"
cp "$work/x/LICENSE" "$work/x/VERSION" "$dest/"
cp -R "$work/x/licenses" "$dest/"
echo "PDFium $RELEASE ($target) is in $dest"
