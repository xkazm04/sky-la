# Getting started

## Install

Download the bundle for your system from the project's [releases page](https://github.com/xkazm04/sky-la/releases):

| System | File |
|---|---|
| Linux (Debian, Ubuntu) | `sky-la_<version>_amd64.deb`: `sudo apt install ./sky-la_<version>_amd64.deb` |
| Linux (other) | `sky-la_<version>_amd64.AppImage`: make it executable and run it |
| macOS 11 or later | `sky-la_<version>_aarch64.dmg` (Apple silicon) or `_x64.dmg` (Intel) |
| Windows 10 or later | `sky-la_<version>_x64-setup.exe`, which installs for your user only |

To check a download, compare its SHA-256 with the line in the release's `SHA256SUMS`.

## The demo

On first start, sky-la offers to set up your books. **Explore the demo instead** opens the books of an invented OSVČ, Jan Novák, with six months of entries, so you can try everything first. The demo lives in memory: nothing you do in it is saved, and it has no backups.

## Creating your books

1. **Who the books are for:** your name as printed on invoices, IČO, DIČ (required if you're a VAT payer), address, VAT status (monthly, quarterly, or not a payer), your flat-rate expense group, the trade-register line, and your business account. You can correct these later in **Settings → Business details**, except the VAT status, which follows your registration.
2. **A passphrase:** at least 10 characters. A few unrelated words work well. It encrypts your books on this computer. Nobody can reset it, including the project.
3. **The recovery key:** shown **once**. Write it down or print it and keep it away from this computer. sky-la then asks for its last group to make sure you saved it.

Your books are now one encrypted file in your user's application-data folder, with a Backups folder beside it.

## Unlocking

Each start asks for your passphrase. If you tick **Remember on this computer**, the key is kept in your system keychain (macOS Keychain, Windows Credential Manager, the Secret Service on Linux), and the books open without asking.

Your books lock after 15 minutes without a key press or pointer move, and whenever you choose **Lock now** in Settings → Encryption. Unlocking them again always asks for the passphrase.

## If you forget the passphrase

Choose **Forgot the passphrase?** on the unlock screen, enter the recovery key, and choose a new passphrase. The recovery key you used stops working and a **new one is shown once**, so write it down again.
