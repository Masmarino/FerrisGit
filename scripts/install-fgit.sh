#!/bin/sh
# Installs fgit on Linux or macOS, from the binaries attached to a GitHub release:
#
#   curl -fsSL https://raw.githubusercontent.com/Masmarino/FerrisGit/main/scripts/install-fgit.sh | sh
#
# FGIT_VERSION picks a release (1.2.3, the latest by default) and FGIT_INSTALL_DIR the folder (~/.local/bin by default).
# Running it again upgrades. It needs no root and only touches that one file.
set -eu

REPOSITORY="Masmarino/FerrisGit"

fail() {
    echo "error: $1" >&2
    exit 1
}

target_triple() {
    case "$(uname -s)" in
        Linux) os="unknown-linux-musl" ;;
        Darwin) os="apple-darwin" ;;
        *) fail "unsupported system $(uname -s). On Windows, use install-fgit.ps1" ;;
    esac
    case "$(uname -m)" in
        x86_64 | amd64) arch="x86_64" ;;
        arm64 | aarch64) arch="aarch64" ;;
        *) fail "unsupported processor $(uname -m)" ;;
    esac
    # A shell under Rosetta reports x86_64 on an Apple processor, which runs the native binary just as well.
    if [ "$os" = "apple-darwin" ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = "1" ]; then
        arch="aarch64"
    fi
    echo "$arch-$os"
}

download() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --retry 3 -o "$2" "$1"
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O "$2" "$1"
    else
        fail "neither curl nor wget is installed"
    fi
}

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        fail "neither sha256sum nor shasum is installed, so the download can't be checked"
    fi
}

main() {
    target="$(target_triple)"
    base="${FGIT_BASE_URL:-https://github.com/$REPOSITORY/releases}"
    if [ -n "${FGIT_VERSION:-}" ]; then
        release="$base/download/v${FGIT_VERSION#v}"
        wanted="version ${FGIT_VERSION#v}"
    else
        release="$base/latest/download"
        wanted="the latest release"
    fi
    install_dir="${FGIT_INSTALL_DIR:-$HOME/.local/bin}"
    archive="fgit-$target.tar.gz"

    work="$(mktemp -d)"
    trap 'rm -rf "$work"' EXIT

    echo "Downloading fgit for $target ($wanted)..."
    download "$release/$archive" "$work/$archive" ||
        fail "could not download $archive from $wanted: check the version, and that this release has fgit binaries"
    download "$release/fgit-checksums.txt" "$work/checksums.txt" ||
        fail "could not download the checksums of $wanted, so the download can't be checked"

    expected="$(awk -v name="$archive" '{ file = $2; sub(/^\*/, "", file) } file == name { print $1 }' "$work/checksums.txt")"
    [ -n "$expected" ] || fail "$archive isn't in the release's checksums"
    actual="$(sha256_of "$work/$archive")"
    [ "$actual" = "$expected" ] || fail "the checksum of $archive doesn't match: nothing was installed"

    tar xzf "$work/$archive" -C "$work" fgit || fail "$archive can't be unpacked"

    # Beside the target and renamed over it, so that an fgit in use or an interrupted copy can't leave a broken one. It is
    # tried there and not in the temporary folder, which is often mounted noexec on a server.
    mkdir -p "$install_dir"
    candidate="$install_dir/.fgit.new"
    cp "$work/fgit" "$candidate"
    chmod 755 "$candidate"
    if ! "$candidate" --version >/dev/null 2>&1; then
        rm -f "$candidate"
        fail "the downloaded fgit doesn't run from $install_dir: nothing was installed"
    fi
    mv -f "$candidate" "$install_dir/fgit"

    echo "Installed $("$install_dir/fgit" --version) in $install_dir"
    case ":$PATH:" in
        *":$install_dir:"*) echo "Next: fgit auth login --hostname <your instance>" ;;
        *)
            echo
            echo "$install_dir isn't on your PATH. Add this line to your shell's configuration, then open a new terminal:"
            echo "  export PATH=\"$install_dir:\$PATH\""
            ;;
    esac
}

# Last, and all in functions: a download cut short runs nothing rather than half of this.
main
