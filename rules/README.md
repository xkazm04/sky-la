# Rule packs

A rule pack holds every statutory value sky-la calculates with for one jurisdiction and year: rates, thresholds, rounding, deadlines, VAT codes and their return rows, public holidays. The engine never hard-codes these. It asks the pack, and every value carries the provision it comes from. Packs are Apache-2.0, like the crate that reads them (`crates/skyla-rules`).

```
rules/
  _template/            a complete, fictional pack to copy (jurisdiction "XX")
  cz/
    chart.toml          the Czech chart of accounts
    2026/
      pack.toml         the values
      golden.toml       what the pack must answer, worked out by hand
```

## Adding or changing a pack

1. **Start from the template.** For a new year or country, copy `rules/_template/` to `rules/<cc>/<year>/`. For a change, edit the pack in place and raise `version` (`2026.1` → `2026.2`). An update is only accepted when its version is higher.
2. **Write the values,** following the citation rules below. Every `[[value]]`, `[[vat_code]]`, `[[obligation]]` and `[[holiday]]` needs a `cite`.
3. **Write the golden cases** in `golden.toml`, worked out from the law, not copied from `pack.toml`. Every value key, VAT code, obligation and holiday needs at least one case.
4. **Check it:** `just pack-check rules/<cc>/<year>` validates the pack and runs its golden cases; `just test` runs them for every pack.
5. **Open a pull request** that names the sources you used. A second person re-derives the golden cases from the official texts before `review` changes from `"draft"` to `"reviewed"`.

## Citation rules

- **Cite the provision, precisely.** `cite = { act = "zdph", section = "§ 47 odst. 1 písm. a)" }`, not just the act. When the wording changed, say which: `"§ 47 odst. 1 písm. b), ve znění zákona č. 349/2023 Sb."`.
- **Every act is listed once** in `[[act]]`, with its official name and an `https` URL, ideally of the consolidated text (for Czech law, zakonyprolidi.cz or e-Sbírka).
- **Date every value.** `effective_from` is the first day it applies. When a value changes, close the old entry with `effective_to` and add a new one, so past periods still compute with what applied then. Periods of one key must not overlap.
- **Don't guess.** A value you can't verify goes into `[pack].omitted`, with what's missing ("needs the 2026 average-wage regulation"). The app says "not assessed" rather than compute with a guess.
- **Values only, no logic.** A pack says *what* the law sets (15 %, 25 days, round half up). How it's applied lives in the engine (`crates/skyla-tax-cz`), with its own tests.
- **Notes are for people.** `note` explains a value in a sentence; the app shows it beside the citation.

`Pack::from_toml` refuses a pack that breaks these rules mechanically: an unknown act, a section left empty, an undated or overlapping value, a value that doesn't parse as its kind, a VAT code pointing at a rate that isn't a percent, an obligation counting from a value that isn't a number of days. `just pack-check` prints every problem at once.

## The format

| Section | What it holds |
|---|---|
| `[pack]` | `id` (`<cc>-<year>`), `version`, `jurisdiction`, `valid_from`/`valid_to`, `review` (`draft` or `reviewed`), `currency`, `summary`, `omitted` |
| `[[act]]` | `id`, `name`, `url` |
| `[[value]]` | `key`, `kind` (`percent`, `amount`, `days`, `rounding`, `flag`), `value`, `effective_from`, `effective_to`, `cite`, `note` |
| `[[obligation]]` | `id`, `name`, `action` (`file`, `pay`, `file_and_pay`), `applies_to` (entity facts: `osvc`, `vat_monthly`, `vat_quarterly`), `frequency` (`monthly`, `quarterly`, `yearly`), `due` (`days_after_period` = a days key, or `month_offset` + `day`), `cite`, `note` |
| `[[vat_code]]` | `code`, `name`, `rate` (a percent key), `cite`, `rows` (return rows its `base` and `tax` feed), `outside_vat`, `einvoice` (EN 16931 category and exemption reason) |
| `[[holiday]]` | `name`, `date` (`MM-DD`) or `easter_offset` (days from Easter Sunday), `cite` |

Amounts are written in the pack's currency with a decimal point (`2000000.00`) and are integer minor units inside the engine. Rounding is `half_up`, `half_even`, `toward_zero` or `away_from_zero`. The template shows each section with comments.

## Golden cases

`golden.toml` is how a reviewer checks a pack, and how CI keeps it checked:

```toml
[[value]]
key = "vat.rate.reduced"
on = "2026-06-30"
expect = "12"

[[working_day]]
date = "2026-04-03"
expect = false                 # Velký pátek

[[deadline]]
obligation = "vat.return.monthly"
period = "2026-03"
due = "2026-04-27"             # 31 March + 25 days is a Saturday, so Monday

[[vat_code]]
code = "OUT12"
on = "2023-06-30"
expect = "15"                  # before zákon č. 349/2023 Sb.
```

Work each case out from the cited text and a calendar, and say why in a comment when it isn't obvious: a shifted deadline, a rate that changed mid-year, a holiday on a weekend. A deadline case names the obligation's period as the calendar labels it: `2026-03` (monthly), `2026-Q1` (quarterly), `2025` (yearly).

When a reviewer believes the pack is wrong but the fix needs a decision (it changes other results), the case records it with `open`:

```toml
[[value]]
key = "insurance.social.share"
on = "2026-06-30"
expect = "55"
open = "§ 5b odst. 1 ZOS as amended by zákon č. 349/2023 Sb.: 55 % of the profit from 2024"
```

An open case is reported, not failed. Once the pack agrees, the harness fails until the `open` note is removed, so findings don't linger after they're fixed.

## Signed updates

Installed copies of sky-la can take a pack update without a new release: the pack's TOML and a minisign signature over its exact bytes, checked against the keys in `TRUSTED_PACK_KEYS` (`crates/skyla-app/src/core/refdata.rs`). The list is empty until the maintainer creates the pack-signing key (`docs/RELEASING.md`), so until then packs change only with a release.
