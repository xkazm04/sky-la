# Your data

## Where it lives

Your books are one SQLCipher-encrypted file in your user's application-data folder. A random data key encrypts it, and your passphrase (through Argon2id) and your recovery key each unlock that key. Nothing about your books leaves the computer, except what you send an advisor (see [Advisors](advisors.md)) and the files you export.

sky-la makes no network connections of its own, except for two opt-in checks that are off by default: **Public reference data** (ČNB exchange rates, from www.cnb.cz) and **Updates** (the project's release page on GitHub). Neither sends anything about you. There is no telemetry.

## Backups

sky-la backs up every day, when you open the books, to the **Backups** folder beside them, and keeps the newest few. Each backup is encrypted with the same key, so your passphrase and your recovery key both open it. Beside each one, a manifest records its content hash and the journal's hash-chain head, so a rewritten history can't pass as a backup.

**Settings → Backups** lists them, makes one now, and **checks the newest backup**: sky-la restores it into a private folder, opens it, and confirms that its content and chain head match the manifest. Copy the Backups folder to another disk now and then.

## Export everything

**Settings → Export everything** saves one zip, after asking for your passphrase:

- `journal.json`: every posted entry with its lines and hash-chain link;
- `journal.csv`: one row per posting;
- `accounts.csv`: the chart of accounts;
- `documents/`: each issued document as ISDOC and PDF;
- `manifest.json`: the chain head and a SHA-256 of every file.

The zip isn't encrypted. Keep it as safe as the books themselves. The same books always give the same file, byte for byte.

## Moving in from Pohoda or Fakturoid

**Invoices → Import…** reads issued invoices from a Pohoda XML export or a Fakturoid CSV export, and shows a preview before anything changes:

- **Will be imported:** new invoices, each to be posted as one entry (receivable, revenue, and VAT at the rule pack's rate on its tax point) and kept as an issued document with its original number.
- **Won't be imported**, with the reason: dated outside your books' periods, VAT that doesn't match the rate, an invalid IČO, another currency, or a credit note (not imported yet; enter it against the imported invoice).
- **Already in these books:** skipped.

Payments aren't imported: import your bank statements and they settle the invoices as usual.
