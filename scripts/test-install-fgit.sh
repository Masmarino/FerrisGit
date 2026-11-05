#!/bin/sh
# shellcheck disable=SC2016 # The checks are strings that `check` evaluates later, on purpose.
# Tests install-fgit.sh and render-fgit-formula.sh against a fake release on a local server, with a fake fgit, so that it
# needs neither a build nor a published release. Needs python3.
set -eu

scripts_dir="$(cd "$(dirname "$0")" && pwd)"
port="${FGIT_TEST_PORT:-18765}"
root="$(mktemp -d)"
server_pid=""
cleanup() {
    [ -z "$server_pid" ] || kill "$server_pid" 2>/dev/null || true
    rm -rf "$root"
}
trap cleanup EXIT

failures=0
check() {
    if eval "$2"; then
        echo "ok: $1"
    else
        echo "FAILED: $1"
        failures=$((failures + 1))
    fi
}

case "$(uname -s)" in
    Linux) os="unknown-linux-musl" ;;
    Darwin) os="apple-darwin" ;;
    *) echo "this test is for Linux and macOS" >&2; exit 2 ;;
esac
case "$(uname -m)" in
    x86_64 | amd64) arch="x86_64" ;;
    *) arch="aarch64" ;;
esac
if [ "$os" = "apple-darwin" ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = "1" ]; then
    arch="aarch64"
fi
target="$arch-$os"

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'; else shasum -a 256 "$1" | awk '{print $1}'; fi
}

# A release: the fake binary, packed the way the CI does, and its checksums.
make_release() {
    dir="$root/releases/$1"
    mkdir -p "$dir" "$root/pack"
    printf '#!/bin/sh\necho "fgit %s"\n' "$2" > "$root/pack/fgit"
    chmod 755 "$root/pack/fgit"
    tar czf "$dir/fgit-$target.tar.gz" -C "$root/pack" fgit
    printf '%s  %s\n' "$(sha256 "$dir/fgit-$target.tar.gz")" "fgit-$target.tar.gz" > "$dir/fgit-checksums.txt"
}

make_release latest/download 2.0.0
make_release download/v1.0.0 1.0.0
# One whose archive isn't what its checksums say, and one with no fgit at all.
make_release download/v6.6.6 6.6.6
printf 'tampered' >> "$root/releases/download/v6.6.6/fgit-$target.tar.gz"
mkdir -p "$root/releases/download/v0.1.0"

(cd "$root" && exec python3 -m http.server "$port" --bind 127.0.0.1 >/dev/null 2>&1) &
server_pid=$!
i=0
until python3 -c "import urllib.request as u; u.urlopen('http://127.0.0.1:$port/releases/latest/download/fgit-checksums.txt')" 2>/dev/null; do
    i=$((i + 1))
    [ "$i" -lt 50 ] || { echo "the test server didn't start" >&2; exit 1; }
    sleep 0.1
done

export FGIT_BASE_URL="http://127.0.0.1:$port/releases"
install_dir="$root/home/bin"
run_install() {
    (FGIT_INSTALL_DIR="$install_dir" FGIT_VERSION="${1:-}" sh "$scripts_dir/install-fgit.sh") > "$root/out" 2>&1
}

check "the latest release installs" 'run_install && [ "$("$install_dir/fgit")" = "fgit 2.0.0" ]'
check "it says the folder isn't on the PATH" 'grep -q "on your PATH" "$root/out"'
check "only fgit lands in the folder" '[ "$(ls -A "$install_dir")" = "fgit" ]'

check "a version can be pinned, and running again replaces the binary" 'run_install 1.0.0 && [ "$("$install_dir/fgit")" = "fgit 1.0.0" ]'
check "a version also reads with its v" 'run_install v1.0.0 && [ "$("$install_dir/fgit")" = "fgit 1.0.0" ]'

check "a tampered archive is refused" '! run_install 6.6.6 && grep -q "checksum" "$root/out"'
check "and leaves what was installed" '[ "$("$install_dir/fgit")" = "fgit 1.0.0" ]'

check "a release without binaries is a clear error" '! run_install 0.1.0 && grep -q "has fgit binaries" "$root/out"'
check "and so is an unknown version" '! run_install 9.9.9 && grep -q "check the version" "$root/out"'

check "a folder on the PATH gets the next step instead" '(PATH="$install_dir:$PATH" FGIT_INSTALL_DIR="$install_dir" sh "$scripts_dir/install-fgit.sh") > "$root/out" 2>&1 && grep -q "fgit auth login" "$root/out"'

# The formula, from a checksums file like the release's.
sums="$root/sums.txt"
for t in aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-musl x86_64-unknown-linux-musl; do
    printf '%064d  fgit-%s.tar.gz\n' 0 "$t" >> "$sums"
done
check "the formula renders" 'sh "$scripts_dir/render-fgit-formula.sh" v1.2.3 "$sums" > "$root/fgit.rb"'
check "with the version, without its v" 'grep -q "version \"1.2.3\"" "$root/fgit.rb"'
check "and each platform" '[ "$(grep -c "/download/v1.2.3/fgit-.*\.tar\.gz" "$root/fgit.rb")" = 4 ]'
if command -v ruby >/dev/null 2>&1; then
    check "and is valid Ruby" 'ruby -c "$root/fgit.rb" >/dev/null'
fi
sed '/aarch64-apple-darwin/d' "$sums" > "$root/partial.txt"
check "a missing platform refuses to render" '! sh "$scripts_dir/render-fgit-formula.sh" 1.2.3 "$root/partial.txt" >/dev/null 2>&1'

[ "$failures" -eq 0 ] || { echo "$failures check(s) failed" >&2; exit 1; }
echo "all checks passed"
