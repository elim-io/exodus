#!/bin/sh
# Install exodus; with a one-time code from the cabinet, also link the repository you are in to your Exodus account.
#
#   curl -fsSL https://exodus.elim.io/install | sh                       install only
#   curl -fsSL https://exodus.elim.io/install | sh -s -- CODE [--new]    install, then `exodus init --code CODE` here
#
# Downloads the release binary for this machine from GitHub Releases of elim-io/exodus, checks its SHA-256 against
# the release's SHA256SUMS and puts it in ~/.local/bin. With a CODE it then runs `exodus init --code CODE` in the
# current folder, which links it only when it is the top folder of a git repository (or, with --new, an empty folder).
# Anywhere else, the home folder included, it leaves the code unused and prints the command that spends it in the
# right folder (the check is the binary's, from v0.2.0). Every argument after the code goes to `exodus init` (`--new`,
# `--here`, `--api URL`).
#
#   --path              also add the install folder to PATH in your shell's rc file
#   EXODUS_VERSION      a release tag, such as v0.1.0 (default: the latest release)
#   EXODUS_INSTALL_DIR  where to put the binary (default: ~/.local/bin)
#   GITHUB_TOKEN        read through the GitHub API with this token (a private fork, or rate limits); it is passed to
#                       curl on stdin, never on the command line
set -eu

repo="${EXODUS_REPO:-elim-io/exodus}"
version="${EXODUS_VERSION:-latest}"
dir="${EXODUS_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf 'exodus: %s\n' "$*" >&2; }
die() { say "$*"; exit 1; }

code=""
path_line=0
init_args=""
for a in "$@"; do
    case "$a" in
        --path) path_line=1 ;;
        -*) init_args="$init_args $a" ;;
        *) if [ -z "$code" ]; then code="$a"; else init_args="$init_args $a"; fi ;;
    esac
done

command -v curl >/dev/null || die "curl is needed"
command -v tar >/dev/null || die "tar is needed"
if command -v sha256sum >/dev/null; then sum="sha256sum"
elif command -v shasum >/dev/null; then sum="shasum -a 256"
else die "sha256sum or shasum is needed"; fi

os=$(uname -s)
arch=$(uname -m)
case "$arch" in
    x86_64 | amd64) arch=x86_64 ;;
    aarch64 | arm64) arch=aarch64 ;;
    *) die "no release for $os $arch; build from source: https://github.com/$repo" ;;
esac
case "$os" in
    Linux) target="$arch-unknown-linux-musl" ;;
    Darwin)
        # an Intel shell under Rosetta on Apple silicon still gets the native binary
        if [ "$arch" = x86_64 ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null)" = 1 ]; then arch=aarch64; fi
        target="$arch-apple-darwin" ;;
    *) die "no release for $os $arch; build from source: https://github.com/$repo" ;;
esac
archive="exodus-$target.tar.gz"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# curl with the token, if any, read from stdin as a config line
get() {
    if [ -n "${GITHUB_TOKEN:-}" ]; then
        printf 'header = "Authorization: Bearer %s"\n' "$GITHUB_TOKEN" | curl -fsSL --retry 2 -K - "$@"
    else
        curl -fsSL --retry 2 "$@"
    fi
}

if [ -n "${GITHUB_TOKEN:-}" ]; then
    # the API serves a private repository's assets; release/download URLs do not take a token
    if [ "$version" = latest ]; then rel="latest"; else rel="tags/$version"; fi
    get -o "$tmp/release.json" "https://api.github.com/repos/$repo/releases/$rel" \
        || die "no release $version in $repo"
    # each asset's API url comes before its name; split the JSON on commas and pair them
    asset_url() {
        tr ',' '\n' <"$tmp/release.json" | awk -v want="\"name\":\"$1\"" '
            { gsub(/[ \t]/, "") }
            /"url":"https:\/\/api\.github\.com\/repos\/.*\/releases\/assets\/[0-9]+"/ { u = $0; sub(/^.*"url":"/, "", u); sub(/".*$/, "", u) }
            $0 == want && u != "" { print u; exit }'
    }
    fetch() {
        u=$(asset_url "$1")
        [ -n "$u" ] || die "the release has no $1"
        get -H "Accept: application/octet-stream" -o "$tmp/$1" "$u" || die "could not download $1"
    }
else
    if [ "$version" = latest ]; then base="https://github.com/$repo/releases/latest/download"
    else base="https://github.com/$repo/releases/download/$version"; fi
    fetch() { get -o "$tmp/$1" "$base/$1" || die "could not download $base/$1"; }
fi

say "downloading $archive ($version)"
fetch "$archive"
fetch SHA256SUMS
want=$(awk -v f="$archive" '$2 == f || $2 == "*" f { print $1 }' "$tmp/SHA256SUMS")
[ -n "$want" ] || die "SHA256SUMS has no line for $archive"
got=$(cd "$tmp" && $sum "$archive" | awk '{ print $1 }')
[ "$got" = "$want" ] || die "SHA-256 mismatch for $archive: expected $want, got $got"

tar -xzf "$tmp/$archive" -C "$tmp"
mkdir -p "$dir"
# a rename, so a running exodus keeps its old file
cp "$tmp/exodus" "$dir/.exodus.new"
chmod 755 "$dir/.exodus.new"
mv -f "$dir/.exodus.new" "$dir/exodus"
say "installed $("$dir/exodus" --version) to $dir/exodus"

case ":$PATH:" in
    *":$dir:"*) ;;
    *)
        if [ "$path_line" = 1 ]; then
            case "${SHELL:-}" in
                */zsh) rc="$HOME/.zshrc" ;;
                */bash) rc="$HOME/.bashrc" ;;
                *) rc="$HOME/.profile" ;;
            esac
            # shellcheck disable=SC2016 # $PATH is written literally into the rc file
            printf '\nexport PATH="%s:$PATH"\n' "$dir" >>"$rc"
            say "added $dir to PATH in $rc; open a new shell to use it"
        else
            say "$dir is not on PATH, so the command is $dir/exodus; run the installer again with --path to add it"
        fi ;;
esac

[ -n "$code" ] || exit 0
# exec skips the EXIT trap
rm -rf "$tmp"
# init spends the code only in a repository's top folder (or an empty one with --new), else prints where to run it
# shellcheck disable=SC2086 # init_args holds flags split on purpose
exec "$dir/exodus" init --code "$code" $init_args
