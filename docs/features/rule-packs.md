# Rule packs

All statutory data lives in versioned, effective-dated packs with a citation for every value. Logic asks the pack for a value *on a date* and never hard-codes a rate, threshold or deadline.

**Code:** `crates/skyla-rules` (Apache-2.0) · **Data:** `rules/cz/2026/pack.toml`, `rules/cz/chart.toml`, `rules/_template/` · **Contributor guide:** `rules/README.md`

## What the CZ 2026 pack holds

- VAT rates (including the 2024 merger of 15 % and 10 % into 12 %), document rounding, the registration threshold and VAT codes with their DPH rows. Each code also has its EN 16931 category and any exemption reason.
- The kontrolní hlášení itemisation threshold.
- Income tax rate, the taxpayer credit, flat-rate expense percentages and caps, and the fixed-asset threshold.
- Social and health insurance: assessment share and rates.
- The late-interest margin over the ČNB repo rate.
- Public holidays (including Easter) and the weekend/holiday deadline shift.
- `[[obligation]]` entries: DPH, KH, DPFO (on paper and electronically), and social and health advances. Each has its schedule, the facts it applies to (`osvc`, `vat_monthly`, `vat_quarterly`) and a citation.

The pack's status is `draft` until a second person verifies it. Values that weren't certain are listed under `omitted` rather than guessed: the 2026 average wage and minimum assessment bases, the solidarity threshold, sickness insurance, the depreciation groups and the paušální daň bands.

## Loading and validation

`Pack::from_toml` validates the whole pack and reports every problem at once, so a bad pack is refused before anything uses it. `Pack::calendar(year, facts)` expands the obligations into dated deadlines moved to the next working day. For example, 25 October moves to 26 October and Christmas deadlines to 28 December.

## Golden cases

- `rules/cz/2026/golden.toml` has 60 cases worked out by hand from the provisions and a calendar.
- `skyla_rules::golden` runs them and requires every value key, VAT code, obligation and holiday to be covered.
- An `open` case records a known disagreement without failing, and fails once the pack agrees (so a stale note can't linger).
- `skyla-pack check <dir>` (`just pack-check`) prints validation problems, golden failures, gaps and open findings.
- `rules/_template/` is a fictional, valid pack for a jurisdiction "XX". CI tests it, so it stays in step with the format.

**Open finding:** `insurance.social.share` is 50 % in the pack, but § 5b odst. 1 ZOS (as amended by 349/2023 Sb.) sets 55 % from 2024. The fix changes the worked example and the advisor transcripts, so it's recorded as `open` for the maintainer.

## Signed pack updates

- `verify_pack_update` accepts an update only if all of these hold:
  - a trusted minisign key signed its exact bytes;
  - it validates as a pack;
  - it's the same pack id;
  - it's a newer version.
- An installed update is saved in the books and verified again at every opening.
- `TRUSTED_PACK_KEYS` is empty until the maintainer creates the signing key, so updates are refused for now (by design).

## Reference data (opt-in)

`skyla_rules::refdata` parses the ČNB daily rates (`denni_kurz.txt`) and a repo-rate history. The user imports them by hand, or turns on fetching (off by default). The only host is www.cnb.cz. The data keeps its origin, and the repo history feeds statutory late interest.
