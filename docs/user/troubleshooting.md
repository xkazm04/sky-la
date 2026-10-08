# Troubleshooting

**"That passphrase or recovery key doesn't open these books."** Check the keyboard layout and Caps Lock. If the passphrase is lost, use **Forgot the passphrase?** with your recovery key. Without either, the books can't be opened, by anyone. Restoring a backup needs the same passphrase or recovery key.

**The status line says the chain doesn't verify.** A posted entry was changed outside sky-la, or the file is damaged. Don't post more. In Settings → Backups, check the newest backup, then restore it, and tell the project if you can't explain the change.

**A statement won't import: "doesn't tie out".** Its opening balance doesn't match the previous statement's closing balance (or your books on the day before). A statement is probably missing in between; import that one first.

**"Every line in this file was imported before."** You've already imported it, and nothing was added twice.

**An advisor says Claude Code isn't found or isn't signed in.** Install Claude Code and run `claude auth login` in a terminal, then check again in Advisors. sky-la looks for the `claude` command on your PATH.

**An advisor's answer was rejected.** sky-la rejects answers whose numbers don't match its own figures, or whose recommendation isn't one of the engine's scenarios. The run is still in the egress register. Try again, or work from the scenarios themselves.

**A figure looks wrong.** Click it to see the entries behind it, and use **Explain this…**. If a rate, threshold or deadline is wrong, it comes from the rule pack: report it with the provision it should follow, or fix it yourself (see [`rules/README.md`](../../rules/README.md)).

**Where are my files?** The books and the Backups folder are in your user's application-data folder (`~/.local/share/app.skyla.desktop/books` on Linux, `~/Library/Application Support/app.skyla.desktop/books` on macOS, `%APPDATA%\app.skyla.desktop\books` on Windows). Settings → Backups shows the exact folder.
