#!/usr/bin/env bash
# Publish the two tested launcher archives. Requires an authenticated gh CLI and
# a local Ed25519 PKCS#8 DER key matching launcher/elyby-update-key.txt.
# Usage: publish_elyby.sh owner/repository 0.2.24-elyby.3 /private/key.der /path/to/archives
set -euo pipefail
[[ $# == 4 ]] || { echo 'Expected: owner/repository version key.der archive-directory' >&2; exit 2; }
repo=$1
version=$2
key=$3
dist=$(cd "$4" && pwd)
[[ "$repo" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]]
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+-elyby\.[0-9]+$ ]]
tag="launcher@$version"
root=$(cd "$(dirname "$0")/../.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
umask 077
openssl pkey -inform DER -in "$key" -pubout -outform DER > "$work/public.der"
tail -c 32 "$work/public.der" | base64 | tr -d '\n' > "$work/public.txt"
tr -d '\r\n' < "$root/launcher/elyby-update-key.txt" > "$work/expected.txt"
cmp "$work/public.txt" "$work/expected.txt"
for asset in horizon-launcher-linux-x86_64.tar.gz horizon-launcher-windows-x64.zip; do
  test -s "$dist/$asset"
done
(cd "$dist" && sha256sum horizon-launcher-linux-x86_64.tar.gz horizon-launcher-windows-x64.zip) > "$dist/SHA256SUMS"
{ printf 'horizon release %s\n' "$tag"; cat "$dist/SHA256SUMS"; } > "$work/message"
openssl pkeyutl -sign -rawin -inkey "$key" -keyform DER -in "$work/message" -out "$work/signature"
base64 < "$work/signature" | tr -d '\n' > "$dist/SHA256SUMS.sig"
printf '\n' >> "$dist/SHA256SUMS.sig"
# The signed tag must identify the source actually used to build these archives.
git -C "$root" rev-parse --verify "refs/tags/$tag" >/dev/null
git -C "$root" push "https://github.com/$repo.git" "refs/tags/$tag"
gh release create "$tag" --repo "$repo" --verify-tag --latest --title "Horizon Ely.by $version" \
  --notes 'Ely.by online sessions, verified Minecraft resources and signed launcher updates. See ELYBY.md in the archive.' \
  "$dist/horizon-launcher-linux-x86_64.tar.gz" "$dist/horizon-launcher-windows-x64.zip" \
  "$dist/SHA256SUMS" "$dist/SHA256SUMS.sig"
