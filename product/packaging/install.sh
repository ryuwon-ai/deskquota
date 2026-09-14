#!/bin/sh
set -eu

archive_source=
manifest_source=
install_dir=${HOME:+"$HOME/.local/bin"}
path_action=preview

usage() {
    cat <<'EOF'
Usage: install.sh --archive PATH_OR_URL --checksum-manifest PATH_OR_URL [options]

Options:
  --install-dir DIR        User-writable destination (default: $HOME/.local/bin)
  --path-action ACTION     preview or none (default: preview)
  -h, --help               Show this help

No download or release URL is built in. The installer never edits a shell profile.
EOF
}

fail() {
    printf 'llmgw installer: %s\n' "$1" >&2
    exit 1
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --archive)
            [ "$#" -ge 2 ] || fail "missing value for --archive"
            archive_source=$2
            shift 2
            ;;
        --checksum-manifest)
            [ "$#" -ge 2 ] || fail "missing value for --checksum-manifest"
            manifest_source=$2
            shift 2
            ;;
        --install-dir)
            [ "$#" -ge 2 ] || fail "missing value for --install-dir"
            install_dir=$2
            shift 2
            ;;
        --path-action)
            [ "$#" -ge 2 ] || fail "missing value for --path-action"
            path_action=$2
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *) fail "unknown argument: $1" ;;
    esac
done

[ -n "$archive_source" ] || fail "--archive is required"
[ -n "$manifest_source" ] || fail "--checksum-manifest is required"
[ -n "$install_dir" ] || fail "--install-dir is required when HOME is unavailable"
case "$path_action" in
    preview|none) ;;
    *) fail "--path-action must be preview or none" ;;
esac

umask 077
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/llmgw-install.XXXXXX") || fail "cannot create temporary directory"
candidate=
cleanup() {
    [ -z "$candidate" ] || rm -f "$candidate"
    rm -rf "$work_dir"
}
trap cleanup EXIT HUP INT TERM

fetch() {
    source_value=$1
    destination=$2
    case "$source_value" in
        http://*|https://*)
            command -v curl >/dev/null 2>&1 || fail "curl is required for URL inputs"
            curl --fail --location --silent --show-error --output "$destination" "$source_value" \
                || fail "could not fetch $source_value"
            ;;
        *)
            [ -f "$source_value" ] || fail "artifact is missing: $source_value"
            cp "$source_value" "$destination" || fail "could not read $source_value"
            ;;
    esac
}

source_basename() {
    value=${1%%\?*}
    basename "$value"
}

archive_name=$(source_basename "$archive_source")
[ -n "$archive_name" ] || fail "archive name is empty"
archive_file="$work_dir/$archive_name"
manifest_file="$work_dir/checksums.sha256"
fetch "$archive_source" "$archive_file"
fetch "$manifest_source" "$manifest_file"

nonempty_lines=$(awk 'NF { count += 1 } END { print count + 0 }' "$manifest_file")
[ "$nonempty_lines" -eq 1 ] || fail "checksum manifest must contain exactly one entry"
expected_hash=$(awk 'NF { print $1 }' "$manifest_file")
expected_name=$(awk 'NF { print $2 }' "$manifest_file")
expected_name=${expected_name#\*}
case "$expected_hash" in
    *[!0123456789abcdefABCDEF]*|'') fail "checksum manifest has an invalid SHA-256" ;;
esac
[ "${#expected_hash}" -eq 64 ] || fail "checksum manifest has an invalid SHA-256"
expected_hash=$(printf '%s' "$expected_hash" | tr 'ABCDEF' 'abcdef')
[ "$expected_name" = "$archive_name" ] || fail "checksum manifest names a different archive"

if command -v shasum >/dev/null 2>&1; then
    actual_hash=$(shasum -a 256 "$archive_file" | awk '{print $1}')
elif command -v sha256sum >/dev/null 2>&1; then
    actual_hash=$(sha256sum "$archive_file" | awk '{print $1}')
else
    fail "shasum or sha256sum is required"
fi
[ "$actual_hash" = "$expected_hash" ] || fail "archive SHA-256 does not match the manifest"

members="$work_dir/members.txt"
details="$work_dir/details.txt"
tar -tzf "$archive_file" >"$members" || fail "archive cannot be listed"
tar -tvzf "$archive_file" >"$details" || fail "archive details cannot be listed"
[ "$(wc -l <"$members" | tr -d ' ')" -eq "$(sort "$members" | uniq | wc -l | tr -d ' ')" ] \
    || fail "duplicate_archive_entry"
[ "$(wc -l <"$members" | tr -d ' ')" -eq "$(wc -l <"$details" | tr -d ' ')" ] \
    || fail "archive listing mismatch"
awk 'substr($0, 1, 1) != "-" { exit 1 }' "$details" || fail "archive links and non-files are not allowed"
while IFS= read -r member; do
    case "$member" in
        llmgw|README.md|docs/installation.md|docs/runtime-contract.md|docs/client-compatibility.md) ;;
        *) fail "unexpected_archive_entry: $member" ;;
    esac
done <"$members"
[ "$(awk '$0 == "llmgw" { count += 1 } END { print count + 0 }' "$members")" -eq 1 ] \
    || fail "archive must contain exactly one llmgw binary"

staged_binary="$work_dir/llmgw"
tar -xOzf "$archive_file" llmgw >"$staged_binary" || fail "could not extract llmgw"
[ -s "$staged_binary" ] || fail "llmgw binary is empty"
chmod 755 "$staged_binary" || fail "could not make staged binary executable"

mkdir -p "$install_dir" || fail "cannot create install directory: $install_dir"
[ -d "$install_dir" ] || fail "install destination is not a directory: $install_dir"
target="$install_dir/llmgw"
[ ! -L "$target" ] || fail "refusing to replace a symbolic-link destination"
[ ! -e "$target" ] || [ -f "$target" ] || fail "refusing to replace a non-file destination"
candidate=$(mktemp "$install_dir/.llmgw-install.XXXXXX") || fail "install directory is not writable"
cp "$staged_binary" "$candidate" || fail "could not stage binary in install directory"
chmod 755 "$candidate" || fail "could not set installed executable permissions"
mv -f "$candidate" "$target" || fail "could not replace installed binary"
candidate=
printf 'Installed verified llmgw at %s\n' "$target"

quoted_install_dir=$(printf '%s' "$install_dir" | sed "s/'/'\\\\''/g")
path_line="export PATH='$quoted_install_dir':\"\$PATH\""
case ":${PATH:-}:" in
    *:"$install_dir":*) path_effective=yes ;;
    *) path_effective=no ;;
esac

if [ "$path_action" = preview ]; then
    printf 'PATH effective in this process: %s\n' "$path_effective"
    printf 'PATH preview (no profile changed):\n  %s\n' "$path_line"
    printf 'Open a new shell after applying this line, then run: llmgw setup\n'
fi
