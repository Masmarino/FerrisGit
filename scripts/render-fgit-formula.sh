#!/bin/sh
# Prints the Homebrew formula of a release on stdout, from the checksums file attached to it.
#
#   scripts/render-fgit-formula.sh 1.2.3 fgit-checksums.txt
#
# The CI publishes the result to the tap on each version tag. FGIT_BASE_URL only exists for the tests.
set -eu

[ "$#" -eq 2 ] || {
    echo "usage: $0 <version> <checksums file>" >&2
    exit 2
}
version="${1#v}"
checksums="$2"
base="${FGIT_BASE_URL:-https://github.com/Masmarino/FerrisGit/releases}"

# Failing is better than publishing a formula that nobody can install.
sha256_of() {
    sum="$(awk -v name="fgit-$1.tar.gz" '{ file = $2; sub(/^\*/, "", file) } file == name { print $1 }' "$checksums")"
    case "$sum" in
        ????????????????????????????????????????????????????????????????) echo "$sum" ;;
        *)
            echo "error: no checksum for fgit-$1.tar.gz in $checksums" >&2
            return 1
            ;;
    esac
}

macos_arm="$(sha256_of aarch64-apple-darwin)"
macos_intel="$(sha256_of x86_64-apple-darwin)"
linux_arm="$(sha256_of aarch64-unknown-linux-musl)"
linux_intel="$(sha256_of x86_64-unknown-linux-musl)"

cat <<EOF
# Generated on each release by scripts/render-fgit-formula.sh, in the FerrisGit repository: don't edit it here.
class Fgit < Formula
  desc "Command-line client of FerrisGit, for people, scripts and AI agents"
  homepage "https://github.com/Masmarino/FerrisGit"
  version "$version"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "$base/download/v$version/fgit-aarch64-apple-darwin.tar.gz"
      sha256 "$macos_arm"
    end
    on_intel do
      url "$base/download/v$version/fgit-x86_64-apple-darwin.tar.gz"
      sha256 "$macos_intel"
    end
  end

  on_linux do
    on_arm do
      url "$base/download/v$version/fgit-aarch64-unknown-linux-musl.tar.gz"
      sha256 "$linux_arm"
    end
    on_intel do
      url "$base/download/v$version/fgit-x86_64-unknown-linux-musl.tar.gz"
      sha256 "$linux_intel"
    end
  end

  def install
    bin.install "fgit"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/fgit --version")
  end
end
EOF
