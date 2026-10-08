//! sky-la desktop shell. Commands stay thin: each one delegates to
//! [`skyla_app::Core`], which owns every rule. tauri-specta exports their
//! signatures and types to `packages/ipc/src/bindings.ts`.

use skyla_app::dto::{
    AdvisorStatusDto, AppInfo, BalanceSheetDto, BankAllocationDto, BankRuleInputDto,
    BankStatementDto, CashBasisDto, ControlStatementDto, DocumentPdfDto, DocumentXmlDto,
    DunningNoticeDto, EgressPayloadDto, EgressPolicyDto, EgressRunDto, EntityDto, ExplainTargetDto,
    ExplanationDto, FindingDto, IntegrityDto, InvoiceDraftDto, InvoiceDto, InvoiceFormDto,
    JournalEntryDto, ObligationDto, PackUpdateDto, PeriodDto, ProfitAndLossDto, ProposalDto,
    RecurringDraftDto, RecurringTemplateDto, RefDataDto, RulePackDto, TaxAdviceDto,
    TaxProjectionDto, TaxScenariosDto, TrialBalanceDto, VatReturnDto,
};
use skyla_app::dto::{
    BackupDto, BackupsDto, DrillDto, EntitySetupDto, ExportDto, ImportPreviewDto, PurchaseDraftDto,
    PurchaseDto, PurchaseFormDto, RecoveryKeyDto, ReportingPeriodsDto, SessionStateDto,
    UpdateStatusDto,
};
use skyla_app::session::Gate;
use skyla_app::{Core, IpcFailure};
use std::path::Path;

use tauri::{Manager, Runtime, State};
use tauri_specta::{Builder, collect_commands};

type Answer<T> = Result<T, IpcFailure>;

/// Build information.
#[tauri::command]
#[specta::specta]
fn app_info(core: Books) -> AppInfo {
    core.app_info()
}

/// The open entity.
#[tauri::command]
#[specta::specta]
fn entity(core: Books) -> EntityDto {
    core.entity()
}

/// Accounting periods.
#[tauri::command]
#[specta::specta]
fn periods(core: Books) -> Answer<Vec<PeriodDto>> {
    Ok(core.periods()?)
}

/// Profit and loss for a date range.
#[tauri::command]
#[specta::specta]
fn profit_and_loss(core: Books, from: String, to: String) -> Answer<ProfitAndLossDto> {
    Ok(core.profit_and_loss(&from, &to)?)
}

/// Balance sheet as of a date.
#[tauri::command]
#[specta::specta]
fn balance_sheet(core: Books, as_of: String) -> Answer<BalanceSheetDto> {
    Ok(core.balance_sheet(&as_of)?)
}

/// Trial balance.
#[tauri::command]
#[specta::specta]
fn trial_balance(core: Books, from: Option<String>, to: String) -> Answer<TrialBalanceDto> {
    Ok(core.trial_balance(from.as_deref(), &to)?)
}

/// Cash basis (*daňová evidence*) for a date range.
#[tauri::command]
#[specta::specta]
fn cash_basis(core: Books, from: String, to: String) -> Answer<CashBasisDto> {
    Ok(core.cash_basis(&from, &to)?)
}

/// Posted journal entries in a date range.
#[tauri::command]
#[specta::specta]
fn journal(core: Books, from: String, to: String) -> Answer<Vec<JournalEntryDto>> {
    Ok(core.journal(&from, &to)?)
}

/// Hash chain and balance, for the status line.
#[tauri::command]
#[specta::specta]
fn integrity(core: Books) -> Answer<IntegrityDto> {
    Ok(core.integrity()?)
}

/// Invoices.
#[tauri::command]
#[specta::specta]
fn invoices(core: Books) -> Answer<Vec<InvoiceDto>> {
    Ok(core.invoices()?)
}

/// The latest bank import with its tie-out.
#[tauri::command]
#[specta::specta]
fn bank_statement(core: Books) -> Answer<BankStatementDto> {
    Ok(core.bank_statement()?)
}

/// The inbox.
#[tauri::command]
#[specta::specta]
fn proposals(core: Books) -> Answer<Vec<ProposalDto>> {
    Ok(core.proposals()?)
}

/// The egress register.
#[tauri::command]
#[specta::specta]
fn egress_register(core: Books) -> Answer<Vec<EgressRunDto>> {
    Ok(core.egress_register()?)
}

/// Exactly what one run sent ("What was shared").
#[tauri::command]
#[specta::specta]
fn egress_payload(core: Books, id: String) -> Answer<EgressPayloadDto> {
    Ok(core.egress_payload(&id)?)
}

/// Each advisor task, what it may send, and the user's policy.
#[tauri::command]
#[specta::specta]
fn egress_policies(core: Books) -> Vec<EgressPolicyDto> {
    core.egress_policies()
}

/// Sets a task's policy: `always`, `ask` or `never`.
#[tauri::command]
#[specta::specta]
fn set_egress_policy(core: Books, task: String, policy: String) -> Answer<Vec<EgressPolicyDto>> {
    Ok(core.set_egress_policy(&task, &policy)?)
}

/// The DPH return for a period, mapped by the rule pack.
#[tauri::command]
#[specta::specta]
fn vat_return(core: Books, from: String, to: String) -> Answer<VatReturnDto> {
    Ok(core.vat_return(&from, &to)?)
}

/// The kontrolní hlášení for a period, checked against the return.
#[tauri::command]
#[specta::specta]
fn control_statement(core: Books, from: String, to: String) -> Answer<ControlStatementDto> {
    Ok(core.control_statement(&from, &to)?)
}

/// The § 7 scenarios, from the books or the user's projection.
#[tauri::command]
#[specta::specta]
fn income_tax_scenarios(
    core: Books,
    projection: Option<TaxProjectionDto>,
) -> Answer<TaxScenariosDto> {
    Ok(core.income_tax_scenarios(projection.as_ref())?)
}

/// Asks the tax advisor to explain the scenarios; every figure is checked
/// against the engine. `confirmed` answers the "ask before each run" policy.
#[tauri::command]
#[specta::specta]
fn run_tax_advisor(
    core: Books,
    projection: Option<TaxProjectionDto>,
    confirmed: bool,
) -> Answer<TaxAdviceDto> {
    Ok(core.run_tax_advisor(projection.as_ref(), confirmed)?)
}

/// The financial advisor's findings for the last complete quarter.
#[tauri::command]
#[specta::specta]
fn financial_findings(core: Books) -> Answer<Vec<FindingDto>> {
    Ok(core.financial_findings()?)
}

/// "Explain this" on an account or an entry, citing the entries behind it.
#[tauri::command]
#[specta::specta]
fn explain(core: Books, target: ExplainTargetDto, confirmed: bool) -> Answer<ExplanationDto> {
    Ok(core.explain(&target, confirmed)?)
}

/// The obligations calendar for a year, from the pack.
#[tauri::command]
#[specta::specta]
fn obligations(core: Books, year: i32) -> Answer<Vec<ObligationDto>> {
    Ok(core.obligations(year)?)
}

/// Whether advisors can run, and what to do if not.
#[tauri::command]
#[specta::specta]
fn advisor_status(core: Books) -> AdvisorStatusDto {
    core.advisor_status()
}

/// The rule pack in force, with citations.
#[tauri::command]
#[specta::specta]
fn rule_pack(core: Books) -> RulePackDto {
    core.rule_pack()
}

/// A document rendered to PDF (`cs` or `en`), base64-encoded.
#[tauri::command]
#[specta::specta]
fn invoice_pdf(core: Books, id: i64, lang: String) -> Answer<DocumentPdfDto> {
    Ok(core.invoice_pdf(id, &lang)?)
}

/// An issued document as XML: `isdoc`, `ubl` (Peppol BIS 3.0) or `cii`.
#[tauri::command]
#[specta::specta]
fn invoice_xml(core: Books, id: i64, format: String) -> Answer<DocumentXmlDto> {
    Ok(core.invoice_xml(id, &format)?)
}

/// Reminders due on a date, drafted for the user to send.
#[tauri::command]
#[specta::specta]
fn dunning_queue(core: Books, as_of: String) -> Answer<Vec<DunningNoticeDto>> {
    Ok(core.dunning_queue(&as_of)?)
}

/// Recurring invoice templates and their next runs.
#[tauri::command]
#[specta::specta]
fn recurring_templates(core: Books) -> Answer<Vec<RecurringTemplateDto>> {
    Ok(core.recurring_templates()?)
}

/// What the invoice editor offers.
#[tauri::command]
#[specta::specta]
fn invoice_form(core: Books) -> Answer<InvoiceFormDto> {
    Ok(core.invoice_form()?)
}

/// Saves a draft typed in the editor; the core parses and checks it.
#[tauri::command]
#[specta::specta]
fn create_invoice_draft(core: Books, draft: InvoiceDraftDto) -> Answer<InvoiceDto> {
    Ok(core.create_invoice_draft(&draft)?)
}

/// Issues a draft: the next number, posted through the kernel.
#[tauri::command]
#[specta::specta]
fn issue_invoice(core: Books, id: i64, issue_date: String) -> Answer<InvoiceDto> {
    Ok(core.issue_invoice(id, &issue_date)?)
}

/// Deletes a draft.
#[tauri::command]
#[specta::specta]
fn delete_invoice_draft(core: Books, id: i64) -> Answer<()> {
    Ok(core.delete_invoice_draft(id)?)
}

/// Imports a statement file (base64): tied out and deduplicated first.
#[tauri::command]
#[specta::specta]
fn import_bank_statement(
    core: Books,
    file_name: String,
    content_base64: String,
) -> Answer<BankStatementDto> {
    Ok(core.import_bank_statement(&file_name, &content_base64)?)
}

/// What importing invoices from Pohoda or Fakturoid would do; changes nothing.
#[tauri::command]
#[specta::specta]
fn preview_invoice_import(
    core: Books,
    file_name: String,
    content_base64: String,
) -> Answer<ImportPreviewDto> {
    Ok(core.preview_invoice_import(&file_name, &content_base64)?)
}

/// Posts the new invoices in a Pohoda or Fakturoid export.
#[tauri::command]
#[specta::specta]
fn commit_invoice_import(
    core: Books,
    file_name: String,
    content_base64: String,
) -> Answer<ImportPreviewDto> {
    Ok(core.commit_invoice_import(&file_name, &content_base64)?)
}

/// The periods the screens report on, as of the books' date.
#[tauri::command]
#[specta::specta]
fn reporting_periods(core: Books) -> Answer<ReportingPeriodsDto> {
    Ok(core.reporting_periods()?)
}

/// Makes a recurring invoice template; what's due runs at once.
#[tauri::command]
#[specta::specta]
fn create_recurring(
    core: Books,
    recurring: RecurringDraftDto,
) -> Answer<Vec<RecurringTemplateDto>> {
    Ok(core.create_recurring(&recurring)?)
}

/// Pauses or resumes a recurring template.
#[tauri::command]
#[specta::specta]
fn set_recurring_active(core: Books, id: i64, active: bool) -> Answer<Vec<RecurringTemplateDto>> {
    Ok(core.set_recurring_active(id, active)?)
}

/// The business details as set up.
#[tauri::command]
#[specta::specta]
fn profile(core: Books) -> EntitySetupDto {
    core.profile()
}

/// Corrects the business details: checked, kept, and in use at once.
#[tauri::command]
#[specta::specta]
fn update_profile(
    session: State<'_, Session>,
    core: Books,
    setup: EntitySetupDto,
) -> Answer<EntityDto> {
    core.update_profile(&setup)?;
    let fresh = core.reopen()?;
    let entity = fresh.entity();
    session.replace(fresh);
    Ok(entity)
}

/// What the purchase editor offers.
#[tauri::command]
#[specta::specta]
fn purchase_form(core: Books) -> Answer<PurchaseFormDto> {
    Ok(core.purchase_form()?)
}

/// Every received invoice, with what's paid.
#[tauri::command]
#[specta::specta]
fn purchases(core: Books) -> Answer<Vec<PurchaseDto>> {
    Ok(core.purchases()?)
}

/// Records a received invoice: checked, posted, the supplier kept.
#[tauri::command]
#[specta::specta]
fn record_purchase(core: Books, draft: PurchaseDraftDto) -> Answer<PurchaseDto> {
    Ok(core.record_purchase(&draft)?)
}

/// Whether the opt-in update check is on, and what it last found.
#[tauri::command]
#[specta::specta]
fn update_status(core: Books) -> UpdateStatusDto {
    core.update_status()
}

/// Turns the update check on or off.
#[tauri::command]
#[specta::specta]
fn set_update_check(core: Books, enabled: bool) -> UpdateStatusDto {
    core.set_update_check(enabled)
}

/// Checks the project's signed release manifest for a newer version.
#[tauri::command]
#[specta::specta]
fn check_for_update(core: Books) -> Answer<UpdateStatusDto> {
    Ok(core.check_for_update()?)
}

/// Everything in the books as one reproducible zip (base64). The zip isn't
/// encrypted, so real books ask for the passphrase again first.
#[tauri::command]
#[specta::specta]
fn export_books(
    session: State<'_, Session>,
    core: Books,
    passphrase: Option<String>,
) -> Answer<ExportDto> {
    if !core.is_demo() {
        session
            .gate
            .confirm_passphrase(passphrase.as_deref().unwrap_or_default())?;
    }
    Ok(core.export_books()?)
}

/// Approves inbox postings: the kernel re-validates and posts each one.
#[tauri::command]
#[specta::specta]
fn approve_proposals(core: Books, ids: Vec<String>) -> Answer<Vec<ProposalDto>> {
    Ok(core.approve_proposals(&ids)?)
}

/// Accepts every line the matcher or a rule is certain about.
#[tauri::command]
#[specta::specta]
fn accept_certain_bank_lines(core: Books) -> Answer<BankStatementDto> {
    Ok(core.accept_certain_bank_lines()?)
}

/// Books a line as the user chose: invoices, or account rows (a split).
#[tauri::command]
#[specta::specta]
fn book_bank_line(
    core: Books,
    line: String,
    allocations: Vec<BankAllocationDto>,
) -> Answer<BankStatementDto> {
    Ok(core.book_bank_line(&line, &allocations)?)
}

/// Makes a rule from a line and books the line by it.
#[tauri::command]
#[specta::specta]
fn create_bank_rule(core: Books, line: String, rule: BankRuleInputDto) -> Answer<BankStatementDto> {
    Ok(core.create_bank_rule(&line, &rule)?)
}

/// Reference data loaded, where it came from, and whether fetching is on.
#[tauri::command]
#[specta::specta]
fn reference_data(core: Books) -> Answer<RefDataDto> {
    Ok(core.reference_data()?)
}

/// Imports a ČNB rates file (`cnb_fx`) or a repo-rate history (`cnb_repo`).
#[tauri::command]
#[specta::specta]
fn import_reference_data(
    core: Books,
    kind: String,
    file_name: String,
    text: String,
) -> Answer<RefDataDto> {
    Ok(core.import_reference_data(&kind, &file_name, &text)?)
}

/// Turns fetching from the ČNB on or off (off by default).
#[tauri::command]
#[specta::specta]
fn set_reference_fetch(core: Books, enabled: bool) -> Answer<RefDataDto> {
    Ok(core.set_reference_fetch(enabled)?)
}

/// Fetches the ČNB's rates for a day, only when fetching is on.
#[tauri::command]
#[specta::specta]
fn fetch_cnb_rates(core: Books, date: String) -> Answer<RefDataDto> {
    Ok(core.fetch_cnb_rates(&date)?)
}

/// Checks a signed rule-pack update.
#[tauri::command]
#[specta::specta]
fn install_pack_update(core: Books, pack_toml: String, signature: String) -> Answer<PackUpdateDto> {
    Ok(core.install_pack_update(&pack_toml, &signature)?)
}

/// The session gate and what to do with books once they open: the shell
/// passes a callback that makes the core the app's managed state.
pub struct Session {
    gate: Gate,
    /// The open books; none before unlocking and after locking.
    books: std::sync::RwLock<Option<std::sync::Arc<Core>>>,
}

/// The open books, as a command argument: refuses with `locked` when the
/// books aren't open, so no command runs on locked books.
pub struct Books(std::sync::Arc<Core>);

impl std::ops::Deref for Books {
    type Target = Core;
    fn deref(&self) -> &Core {
        &self.0
    }
}

impl<'de, R: tauri::Runtime> tauri::ipc::CommandArg<'de, R> for Books {
    fn from_command(
        command: tauri::ipc::CommandItem<'de, R>,
    ) -> Result<Self, tauri::ipc::InvokeError> {
        let session: State<'_, Session> = tauri::ipc::CommandArg::from_command(command)?;
        session.books().map(Books).ok_or_else(|| {
            tauri::ipc::InvokeError::from(IpcFailure {
                code: "locked".into(),
                message: "the books are locked; unlock them first".into(),
            })
        })
    }
}

impl specta::function::FunctionArg for Books {
    fn to_datatype(_: &mut specta::Types) -> Option<specta::datatype::DataType> {
        None
    }
}

impl Session {
    /// A session over `gate`, with no books open yet.
    pub fn new(gate: Gate) -> Self {
        Self {
            gate,
            books: std::sync::RwLock::new(None),
        }
    }

    /// The open books, if any.
    pub fn books(&self) -> Option<std::sync::Arc<Core>> {
        self.books.read().ok().and_then(|b| b.clone())
    }

    /// Holds `core` as the open books as it is (the IPC test holds the demo
    /// with its recorded provider).
    pub fn hold(&self, core: Core) {
        if let Ok(mut b) = self.books.write() {
            *b = Some(std::sync::Arc::new(core));
        }
    }

    /// Closes real books: the key and the connection go with the core once
    /// the last command using it finishes. The demo doesn't lock.
    pub fn lock(&self) -> Result<SessionStateDto, IpcFailure> {
        let mut b = self.books.write().map_err(|_| IpcFailure {
            code: "bad_request".into(),
            message: "try again".into(),
        })?;
        if b.as_ref().is_some_and(|c| c.is_demo()) {
            return Err(IpcFailure {
                code: "bad_request".into(),
                message: "the demo keeps its books in memory and doesn't lock".into(),
            });
        }
        *b = None;
        Ok(self.gate.state())
    }

    /// Gets freshly opened books ready: the CLI driver, the shim beside the
    /// app, a backup if one is due.
    fn prepare(&self, core: &Core) {
        core.replace_provider(Box::new(skyla_advisor::ClaudeCodeCli::from_system()));
        if let Some(dir) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
        {
            core.set_shim_path(dir.join(if cfg!(windows) {
                "skyla-mcp.exe"
            } else {
                "skyla-mcp"
            }));
        }
        if !core.is_demo() {
            // A failed backup mustn't keep the books closed; Settings shows the state.
            let _ = core.backup_if_due(&self.gate.now());
        }
    }

    /// Swaps in the same books reopened (after their details changed).
    pub fn replace(&self, core: Core) {
        self.prepare(&core);
        if let Ok(mut b) = self.books.write() {
            *b = Some(std::sync::Arc::new(core));
        }
    }

    /// Gets freshly opened books ready and holds them, unless books are
    /// open already.
    pub fn adopt(&self, core: Core) -> SessionStateDto {
        self.prepare(&core);
        // Only one set of books is open at a time; the first stays.
        if let Ok(mut b) = self.books.write() {
            if let Some(open) = b.as_ref() {
                return open.session_state();
            }
            let state = core.session_state();
            *b = Some(std::sync::Arc::new(core));
            return state;
        }
        core.session_state()
    }
}

/// Whether books are open, and if not, whether to set up or unlock.
#[tauri::command]
#[specta::specta]
fn session_state(session: State<'_, Session>) -> SessionStateDto {
    match session.books() {
        Some(open) => open.session_state(),
        None => session.gate.state(),
    }
}

/// Locks real books (after a while idle, or on request): the next command
/// needs the passphrase again.
#[tauri::command]
#[specta::specta]
fn lock(session: State<'_, Session>) -> Answer<SessionStateDto> {
    session.lock()
}

/// Creates new books protected by `passphrase`; returns the recovery key to show once.
#[tauri::command]
#[specta::specta]
fn create_entity(
    session: State<'_, Session>,
    setup: EntitySetupDto,
    passphrase: String,
) -> Answer<RecoveryKeyDto> {
    let (core, key) = session.gate.create(&setup, &passphrase)?;
    session.adopt(core);
    Ok(key)
}

/// Checks the user saved the recovery key, by its last group.
#[tauri::command]
#[specta::specta]
fn confirm_recovery_key(session: State<'_, Session>, last_group: String) -> Answer<bool> {
    Ok(session.gate.confirm_recovery_key(&last_group)?)
}

/// Unlocks with the passphrase; `remember` keeps the key in the OS keychain.
#[tauri::command]
#[specta::specta]
fn unlock(
    session: State<'_, Session>,
    passphrase: String,
    remember: bool,
) -> Answer<SessionStateDto> {
    let core = session.gate.unlock(&passphrase, remember)?;
    let state = core.session_state();
    session.adopt(core);
    Ok(state)
}

/// Opens the books with the recovery key and a new passphrase; returns the new recovery key.
#[tauri::command]
#[specta::specta]
fn recover(
    session: State<'_, Session>,
    recovery_key: String,
    new_passphrase: String,
) -> Answer<RecoveryKeyDto> {
    let (core, key) = session.gate.recover(&recovery_key, &new_passphrase)?;
    session.adopt(core);
    Ok(key)
}

/// Opens the demo books instead.
#[tauri::command]
#[specta::specta]
fn open_demo(session: State<'_, Session>) -> Answer<SessionStateDto> {
    if let Some(open) = session.books() {
        return Ok(open.session_state());
    }
    Ok(session.adopt(Core::demo()?))
}

/// The backups there are, and the policy.
#[tauri::command]
#[specta::specta]
fn backups(core: Books) -> Answer<BackupsDto> {
    Ok(core.backups()?)
}

/// Backs up now.
#[tauri::command]
#[specta::specta]
fn backup_now(core: Books) -> Answer<BackupDto> {
    Ok(core.backup_now(&skyla_app::session::system_now())?)
}

/// Restores the newest backup into a scratch folder and checks it.
#[tauri::command]
#[specta::specta]
fn restore_drill(core: Books) -> Answer<DrillDto> {
    Ok(core.restore_drill()?)
}

/// Every command, for the invoke handler and the TypeScript export.
pub fn specta_builder<R: Runtime>() -> Builder<R> {
    Builder::<R>::new()
        .commands(collect_commands![
            session_state,
            lock,
            create_entity,
            confirm_recovery_key,
            unlock,
            recover,
            open_demo,
            backups,
            backup_now,
            restore_drill,
            app_info,
            entity,
            periods,
            profit_and_loss,
            balance_sheet,
            trial_balance,
            cash_basis,
            journal,
            integrity,
            invoices,
            bank_statement,
            proposals,
            egress_register,
            egress_payload,
            egress_policies,
            set_egress_policy,
            vat_return,
            control_statement,
            income_tax_scenarios,
            run_tax_advisor,
            financial_findings,
            explain,
            obligations,
            advisor_status,
            rule_pack,
            invoice_pdf,
            invoice_xml,
            dunning_queue,
            recurring_templates,
            invoice_form,
            create_invoice_draft,
            issue_invoice,
            delete_invoice_draft,
            import_bank_statement,
            preview_invoice_import,
            commit_invoice_import,
            export_books,
            update_status,
            purchase_form,
            reporting_periods,
            profile,
            update_profile,
            create_recurring,
            set_recurring_active,
            purchases,
            record_purchase,
            set_update_check,
            check_for_update,
            approve_proposals,
            accept_certain_bank_lines,
            book_bank_line,
            create_bank_rule,
            reference_data,
            import_reference_data,
            set_reference_fetch,
            fetch_cnb_rates,
            install_pack_update,
        ])
        // Money crosses as integer minor units; `MoneyDto` refuses anything
        // beyond 2^53 - 1, so a JavaScript number holds every value exactly.
        .dangerously_cast_bigints_to_number()
}

/// Header of the generated bindings.
pub const BINDINGS_HEADER: &str = "// Generated by tauri-specta from apps/desktop/src-tauri (`just bindings`). Don't edit.\n// biome-ignore-all lint: generated";

/// Writes the TypeScript bindings to `path`.
///
/// # Errors
/// If the export fails or the file can't be written.
pub fn export_bindings(path: &std::path::Path) -> Result<(), specta_typescript::Error> {
    specta_builder::<tauri::Wry>().export(
        specta_typescript::Typescript::default().header(BINDINGS_HEADER),
        path,
    )
}

/// Whether the app's window may show `url`: only the app's own pages (and
/// the Vite dev server in debug builds). A link to a statute or anything
/// remote never replaces the app, so no remote page can sit in its window
/// looking like an unlock screen.
pub fn navigation_allowed(url: &tauri::Url) -> bool {
    match (url.scheme(), url.host_str()) {
        ("tauri", Some("localhost")) => true,
        ("http" | "https", Some("tauri.localhost")) => true,
        ("http", Some("localhost")) => cfg!(debug_assertions) && url.port() == Some(1420),
        _ => false,
    }
}

/// Starts the desktop application: the session gate first (set up or
/// unlock the books, or explore the demo), then the core.
///
/// # Panics
/// If the Tauri runtime fails to start, there is nothing to recover to.
#[allow(clippy::expect_used)]
pub fn run() {
    let builder = specta_builder::<tauri::Wry>();
    let navigation_guard = tauri::plugin::Builder::<tauri::Wry>::new("navigation-guard")
        .on_navigation(|_, url| navigation_allowed(url))
        .build();
    tauri::Builder::default()
        .plugin(navigation_guard)
        .setup(|app| {
            let dir = app.path().app_data_dir()?.join("books");
            let keystore: Box<dyn skyla_app::KeyStore> = if skyla_store::OsKeyStore::available() {
                Box::new(skyla_store::OsKeyStore::default())
            } else {
                Box::new(skyla_store::MemoryKeyStore::default())
            };
            let session = Session::new(Gate::new(dir, keystore));
            // A remembered key opens the books without asking.
            if let Ok(Some(core)) = session.gate.unlock_remembered() {
                session.adopt(core);
            }
            app.manage(session);
            Ok(())
        })
        .invoke_handler(builder.invoke_handler())
        .run(tauri::generate_context!())
        .expect("error while running the sky-la desktop shell");
}
