# Inbox

Everything that waits for the user's decision. Nothing in the inbox is in the books until a person approves it.

**Code:** `crates/skyla-app/src/core.rs` (`proposals`), `crates/skyla-app/src/core/inbox.rs` · **UI:** `apps/desktop/src/screens/Inbox.tsx` · **Tests:** `crates/skyla-app/tests/inbox.rs`, `crates/skyla-app/tests/persist.rs`, e2e in `apps/desktop/e2e/screens.spec.ts`

## What appears

| Kind | Source | The user can… |
|---|---|---|
| Posting | A rule's or an advisor's proposed entry about a bank line | **Approve and post**, or **Book differently** (opens the line in Bank) |
| Posting | An advisor's `propose_entry` (an accrual, say) or `propose_categorisation` (an account for a bank line) | **Approve and post** |
| Posting | A bank line the workbench couldn't place, with no proposal | **Book in Bank** |
| Deadline | The obligations calendar, next 31 days | **Open in Taxes** |
| Advice | Financial-advisor findings (a big swing, a duplicated charge, a short runway), tax-advisor scenarios, advisor questions (`ask_user`) and findings | **Dismiss**, or open Taxes or Advisors |

**Approve N certain** approves every posting marked certain in one go.

## How approval works

`Core::approve_proposals(ids)` posts each proposal through the kernel, approved by `user`, in order. The first refusal stops the rest and is shown.

- **A proposal about a bank line books that line.** It's re-checked as it was shown:
  - every account is known;
  - VAT is what the engine computes with the pack;
  - the entry balances;
  - settlements don't exceed what's open;
  - the entry is dated on the line's day.

  The entry keeps the proposal's memo and source kind (`rule` or `advisor`), and the workbench shows "Approved: …" for the line.
- **An advisor's entry** posts exactly as it was proposed and reviewed. The inbox keeps the proposed lines in minor units, not as formatted text. Then it leaves the inbox.
- **Advice and deadlines** are refused, since there's nothing to post.

Advisor tools never post. `propose_entry` and `propose_categorisation` run the entry through the kernel's checks in a rolled-back transaction before filing it, so a bad proposal never reaches the inbox.

## Dismissal and persistence

- `dismiss_proposal` removes advice the user has read. Postings can't be dismissed, only approved or booked in Bank.
- Real books keep what advisors filed and what was dismissed (`app_state` key `inbox`), so a proposal survives auto-lock.
- Advisor item ids keep counting and are never reused.

## Not yet

- Answering an advisor's question in the app; the answer has no field yet.
