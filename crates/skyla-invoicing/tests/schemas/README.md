# Vendored schemas

Used only by tests, to validate what the writers produce.

| File | Source | Licence |
|---|---|---|
| `isdoc-invoice-6.0.2.xsd` | ISDOC 6.0.2 (MVČR, ICT UNIE, SPIS), as published at isdoc.cz; sha256 `fb5de36fe7b5517acb8aa0c7950d91f8e28910e63b8cdcaaf7db0b0f4f0420bb` | Permissive notice in the file header (copy and distribute with the notice kept) |

Validation runs through `xmllint` (libxml2). Tests skip with a notice when
it isn't installed, except in CI, where they fail.
