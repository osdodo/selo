#!/usr/bin/env bash
# Creates a self-signed code-signing identity so TCC grants survive rebuilds.
#
# Ad-hoc signing derives the designated requirement from the cdhash, which changes on every
# build, so macOS treats each rebuild as a different app and silently drops the Accessibility
# / Screen Recording grant. Signing with a certificate makes the requirement
#   identifier "<bundle id>" and certificate leaf = H"<leaf hash>"
# which stays identical across rebuilds as long as this certificate exists.
set -euo pipefail

NAME="${1:-Selo Dev}"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

# `security find-identity` only lists certificates that are *trusted* for code signing, and
# a self-signed certificate is not, yet codesign still signs with it happily. So usability
# is tested by actually signing something.
can_sign() {
  local probe
  probe="$(mktemp)"
  cp /bin/echo "$probe"
  codesign --force --sign "$NAME" "$probe" >/dev/null 2>&1
  local ok=$?
  rm -f "$probe"
  return $ok
}

if can_sign; then
  echo "identity already usable: $NAME"
  exit 0
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

openssl req -x509 -newkey rsa:2048 -sha256 -days 3650 -nodes \
  -keyout "$tmp/key.pem" -out "$tmp/cert.pem" \
  -subj "/CN=$NAME" \
  -addext "basicConstraints=critical,CA:false" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning" 2>/dev/null

openssl pkcs12 -export -legacy -out "$tmp/id.p12" \
  -inkey "$tmp/key.pem" -in "$tmp/cert.pem" \
  -name "$NAME" -passout pass:selo

# -A lets codesign use the private key without a per-use keychain prompt.
security import "$tmp/id.p12" -k "$KEYCHAIN" -P selo -T /usr/bin/codesign -A

can_sign || { echo "imported, but codesign still cannot use it" >&2; exit 1; }
echo "identity ready: $NAME"
