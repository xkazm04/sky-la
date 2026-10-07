#!/usr/bin/env bash
# Validates e-invoices against the official schemas and Schematron:
# UBL 2.1 with the OpenPeppol BIS Billing 3.0 rules (which bundle CEN's
# EN 16931 rules) and CII D16B with CEN's EN 16931 rules.
#
# The KoSIT validator is distributed only through GitHub releases; it runs
# these same artefacts, which we take from Maven Central (pinned, hashed).
#
# Usage: scripts/einvoice/validate.sh <file>...
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
tools="${SKYLA_EINVOICE_TOOLS:-$root/target/einvoice-tools}"
maven="https://repo1.maven.org/maven2"

# path sha256
jars=(
  "net/sf/saxon/Saxon-HE/12.10/Saxon-HE-12.10.jar b571af282f25d7301059f788b9a149aab8b5cdc14ef3d212dc5425d3dcbb9a97"
  "org/xmlresolver/xmlresolver/5.3.3/xmlresolver-5.3.3.jar 1fe4d5b92f708dcdb82dbce12919e0171e6b5ca62c6dca6220483625098feb5f"
  "com/helger/ubl/ph-ubl21/10.2.1/ph-ubl21-10.2.1.jar 0bb13039b0f0983df9cf8c6f98b1f0f0c992bbd9ae2196766573e61607402405"
  "com/helger/cii/ph-cii-d16b/10.1.0/ph-cii-d16b-10.1.0.jar 96de61ced70a5ceb1bcc97dc54f5bb102c3527d7bc27abd1e6a507ed6ade64d5"
  "com/helger/phive/rules/phive-rules-en16931/4.6.3/phive-rules-en16931-4.6.3.jar 9ee9f0e6f90824620a8d6158e3a68e7c07af4072308574bd3be95cd41a21fae4"
  "com/helger/phive/rules/phive-rules-peppol/4.6.3/phive-rules-peppol-4.6.3.jar 8d746eed379b5e9e8cd58530e1b8a7931786d089fb94d785e99b9be142c0431b"
  "com/helger/xsd/ph-xsds-ccts-cct-schemamodule/4.1.0/ph-xsds-ccts-cct-schemamodule-4.1.0.jar ec0e60be5265b09fac75a57a4b284244820c23f0481a1c75802ba4eb6338b0fd"
  "com/helger/xsd/ph-xsds-xmldsig/4.1.0/ph-xsds-xmldsig-4.1.0.jar 66590e38b0994d0fd6cd89b1588a3c96fad3a3c85494cf764783fe0c4acc9c3d"
  "com/helger/xsd/ph-xsds-xades132/4.1.0/ph-xsds-xades132-4.1.0.jar d90bdb2359bbef63bd789af238fb135dd7ada0ba9722f4e2825cd210dada013c"
  "com/helger/xsd/ph-xsds-xades141/4.1.0/ph-xsds-xades141-4.1.0.jar b34b2670774e9de79653767fd667d86823261c153f706c2344889d0702f54e41"
)

mkdir -p "$tools/jars"
for entry in "${jars[@]}"; do
  path="${entry% *}"
  sum="${entry#* }"
  jar="$tools/jars/$(basename "$path")"
  if [[ ! -f "$jar" ]] || ! echo "$sum  $jar" | sha256sum -c --status; then
    curl -sSfL --retry 3 -o "$jar.part" "$maven/$path"
    echo "$sum  $jar.part" | sha256sum -c --quiet
    mv "$jar.part" "$jar"
  fi
done

# Bump when the set of extracted files changes.
marker="$tools/.extracted-2"
if [[ ! -f "$marker" ]]; then
  rm -rf "$tools/ubl" "$tools/cii" "$tools/en16931" "$tools/peppol" "$tools/support" "$tools/x" "$tools"/.extracted*
  mkdir -p "$tools/x"
  (cd "$tools/x" &&
    unzip -q -o ../jars/ph-ubl21-10.2.1.jar 'external/schemas/ubl21/*' &&
    unzip -q -o ../jars/ph-cii-d16b-10.1.0.jar 'external/schemas/d16b/*' &&
    unzip -q -o ../jars/phive-rules-en16931-4.6.3.jar 'external/schematron/1.3.16/cii/*' &&
    unzip -q -o ../jars/phive-rules-peppol-4.6.3.jar 'external/schematron/openpeppol/2026.5/xslt/*' &&
    for j in ../jars/ph-xsds-*.jar; do unzip -q -o "$j" 'schemas/*.xsd'; done)
  mv "$tools/x/external/schemas/ubl21" "$tools/ubl"
  mv "$tools/x/external/schemas/d16b" "$tools/cii"
  mv "$tools/x/external/schematron/1.3.16/cii" "$tools/en16931"
  mv "$tools/x/external/schematron/openpeppol/2026.5/xslt" "$tools/peppol"
  mv "$tools/x/schemas" "$tools/support"
  rm -rf "$tools/x"
  touch "$marker"
fi

java -cp "$tools/jars/Saxon-HE-12.10.jar:$tools/jars/xmlresolver-5.3.3.jar" \
  "$here/Validate.java" "$tools" "$@"
