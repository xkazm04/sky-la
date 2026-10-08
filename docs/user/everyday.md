# Everyday work

## The overview and the inbox

**Overview** shows where the business stands: key figures and results, what clients still owe you and what you owe suppliers, and what's coming up. **Inbox** collects what waits for you: proposals from bank rules and advisors, findings and questions, and the deadlines of the next month from the obligations calendar. Nothing in the inbox is in your books yet. **Approve and post** turns a proposal into a journal entry: the core checks it again (accounts, balance, VAT from the rule pack, an open period) and posts it, and a proposal about a bank line books that line. **Approve N certain** does the same for every proposal marked certain. To book a line another way, **Book differently** opens it in Bank. Advice and deadlines have nothing to post; their button opens the screen where you deal with them, and **Dismiss** removes advice you've read (it stays dismissed). When an advisor suggests an account for a bank line, the suggestion is a posting you approve like any other.

## Invoices

- **New invoice:** pick the client, add lines (quantity, unit, price without VAT, VAT code), and save a draft. Drafts change freely: **Edit** on a draft reopens it in the editor, and the core checks the changes like a new draft.
- **Issue:** assigns the next number in the series and posts the invoice (receivable, revenue, VAT). From then on it can't be edited. A mistake is corrected with a credit note, which keeps the history true.
- **Export:** a Czech PDF with a QR Platba code, or ISDOC, UBL or CII for your client's accounting software.
- **Repeat:** in a new invoice, choose *every month*, *quarter* or *year* and the first date, and it becomes a recurring template. Each time one falls due, sky-la makes the invoice (when you open the books, or at once for dates already past): a draft for you to check, or issued straight away if you tick **Issue each one automatically**. `{month}` in a line or the note becomes that invoice's month. **Recurring** on the Invoices toolbar lists the templates and pauses or resumes them.
- **Import…:** brings in invoices issued in Pohoda or Fakturoid (see [Your data](your-data.md#moving-in-from-pohoda-or-fakturoid)).

The status column shows what each invoice needs: overdue (with days), part-paid, paid, or credited.

## Purchases

**Purchases** lists the invoices your suppliers sent you: what's still to pay and what's paid. **Record received invoice** asks for the supplier (name, IČO, and DIČ, which you need to deduct VAT), their invoice number, the dates, and a line per expense account with its amount without VAT. sky-la works out the VAT with the rule pack's rate on the tax point. If you type the VAT printed on the invoice, it checks that the two agree. Saving posts the invoice (expenses, VAT to deduct, the amount owed) and lists it in the kontrolní hlášení with the supplier's DIČ. When you import the statement with the payment, the bank settles it.

## Bank statements

**Bank → Import…** reads a statement file: camt.053 XML, MT940 or ABO/GPC, which most Czech banks export, or Fio's CSV export. sky-la checks that the statement's opening balance ties out to the previous statement (or to your books), and skips lines it has seen before.

Each line then gets a suggestion, with the reasons spelled out (variable symbol, amount, counterparty):

- **Certain** lines settle an invoice exactly, or match a rule you made. **Accept N certain** books them all at once; **Accept** in a line's panel books just that one.
- **Needs you** lines get candidates to choose from, or **Book…** to split a line across accounts with VAT.
- **Create a rule from this line** makes the next similar line certain.

Every accepted line is one journal entry approved by you. If a booking was wrong, **Undo booking…** posts a reversal: the original entry stays in the journal, cancelled by the reversal (dated like the booking, or today if that period is closed), any invoice it settled is open again, and the line waits for a decision.

## Statements and "explain this"

**Statements** shows the profit and loss, the balance sheet, and the cash-basis view for the period you pick, each with the ledger snapshot it was computed from. Click a figure to see the entries behind it. **Explain this…** asks an advisor to explain the figure in words. The answer is accepted only if every number in it is one of the figures shown, and every entry it cites is behind the figure.

## The status line

The bottom line of the window always says whether the journal balances and the hash chain verifies, and what was sent to Claude today. A broken chain means something changed a posted entry outside sky-la; restore from a backup.
