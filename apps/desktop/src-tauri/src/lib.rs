//! sky-la desktop shell. Commands stay thin: each one delegates to
//! [`skyla_app::Core`], which owns every rule. tauri-specta exports their
//! signatures and types to `packages/ipc/src/bindings.ts`.

use skyla_app::dto::{
    AdvisorStatusDto, AppInfo, BalanceSheetDto, BankAllocationDto, BankRuleInputDto,
    BankStatementDto, CashBasisDto, ControlStatementDto, DocumentPdfDto, DocumentXmlDto,
    DunningNoticeDto, EgressPayloadDto, EgressPolicyDto, EgressRunDto, EntityDto, ExplainTargetDto,
    ExplanationDto, FindingDto, IntegrityDto, InvoiceDraftDto, InvoiceDto, InvoiceFormDto,
    JournalEntryDto, ObligationDto, PackUpdateDto, PeriodDto, ProfitAndLossDto, ProposalDto,
    RecurringTemplateDto, RefDataDto, RulePackDto, TaxAdviceDto, TaxProjectionDto, TaxScenariosDto,
    TrialBalanceDto, VatReturnDto,
};
use skyla_app::dto::{
    BackupDto, BackupsDto, DrillDto, EntitySetupDto, ExportDto, ImportPreviewDto, PurchaseDraftDto,
    PurchaseDto, PurchaseFormDto, RecoveryKeyDto, SessionStateDto, UpdateStatusDto,
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
fn app_info(core: State<'_, Core>) -> AppInfo {
    core.app_info()
}

/// The open entity.
#[tauri::command]
#[specta::specta]
fn entity(core: State<'_, Core>) -> EntityDto {
    core.entity()
}

/// Accounting periods.
#[tauri::command]
#[specta::specta]
fn periods(core: State<'_, Core>) -> Answer<Vec<PeriodDto>> {
    Ok(core.periods()?)
}

/// Profit and loss for a date range.
#[tauri::command]
#[specta::specta]
fn profit_and_loss(core: State<'_, Core>, from: String, to: String) -> Answer<ProfitAndLossDto> {
    Ok(core.profit_and_loss(&from, &to)?)
}

/// Balance sheet as of a date.
#[tauri::command]
#[specta::specta]
fn balance_sheet(core: State<'_, Core>, as_of: String) -> Answer<BalanceSheetDto> {
    Ok(core.balance_sheet(&as_of)?)
}

/// Trial balance.
#[tauri::command]
#[specta::specta]
fn trial_balance(
    core: State<'_, Core>,
    from: Option<String>,
    to: String,
) -> Answer<TrialBalanceDto> {
    Ok(core.trial_balance(from.as_deref(), &to)?)
}

/// Cash basis (*daňová evidence*) for a date range.
#[tauri::command]
#[specta::specta]
fn cash_basis(core: State<'_, Core>, from: String, to: String) -> Answer<CashBasisDto> {
    Ok(core.cash_basis(&from, &to)?)
}

/// Posted journal entries in a date range.
#[tauri::command]
#[specta::specta]
fn journal(core: State<'_, Core>, from: String, to: String) -> Answer<Vec<JournalEntryDto>> {
    Ok(core.journal(&from, &to)?)
}

/// Hash chain and balance, for the status line.
#[tauri::command]
#[specta::specta]
fn integrity(core: State<'_, Core>) -> Answer<IntegrityDto> {
    Ok(core.integrity()?)
}

/// Invoices.
#[tauri::command]
#[specta::specta]
fn invoices(core: State<'_, Core>) -> Answer<Vec<InvoiceDto>> {
    Ok(core.invoices()?)
}

/// The latest bank import with its tie-out.
#[tauri::command]
#[specta::specta]
fn bank_statement(core: State<'_, Core>) -> Answer<BankStatementDto> {
    Ok(core.bank_statement()?)
}

/// The inbox.
#[tauri::command]
#[specta::specta]
fn proposals(core: State<'_, Core>) -> Answer<Vec<ProposalDto>> {
    Ok(core.proposals()?)
}

/// The egress register.
#[tauri::command]
#[specta::specta]
fn egress_register(core: State<'_, Core>) -> Answer<Vec<EgressRunDto>> {
    Ok(core.egress_register()?)
}

/// Exactly what one run sent ("What was shared").
#[tauri::command]
#[specta::specta]
fn egress_payload(core: State<'_, Core>, id: String) -> Answer<EgressPayloadDto> {
    Ok(core.egress_payload(&id)?)
}

/// Each advisor task, what it may send, and the user's policy.
#[tauri::command]
#[specta::specta]
fn egress_policies(core: State<'_, Core>) -> Vec<EgressPolicyDto> {
    core.egress_policies()
}

/// Sets a task's policy: `always`, `ask` or `never`.
#[tauri::command]
#[specta::specta]
fn set_egress_policy(
    core: State<'_, Core>,
    task: String,
    policy: String,
) -> Answer<Vec<EgressPolicyDto>> {
    Ok(core.set_egress_policy(&task, &policy)?)
}

/// The DPH return for a period, mapped by the rule pack.
#[tauri::command]
#[specta::specta]
fn vat_return(core: State<'_, Core>, from: String, to: String) -> Answer<VatReturnDto> {
    Ok(core.vat_return(&from, &to)?)
}

/// The kontrolní hlášení for a period, checked against the return.
#[tauri::command]
#[specta::specta]
fn control_statement(
    core: State<'_, Core>,
    from: String,
    to: String,
) -> Answer<ControlStatementDto> {
    Ok(core.control_statement(&from, &to)?)
}

/// The § 7 scenarios, from the books or the user's projection.
#[tauri::command]
#[specta::specta]
fn income_tax_scenarios(
    core: State<'_, Core>,
    projection: Option<TaxProjectionDto>,
) -> Answer<TaxScenariosDto> {
    Ok(core.income_tax_scenarios(projection.as_ref())?)
}

/// Asks the tax advisor to explain the scenarios; every figure is checked
/// against the engine. `confirmed` answers the "ask before each run" policy.
#[tauri::command]
#[specta::specta]
fn run_tax_advisor(
    core: State<'_, Core>,
    projection: Option<TaxProjectionDto>,
    confirmed: bool,
) -> Answer<TaxAdviceDto> {
    Ok(core.run_tax_advisor(projection.as_ref(), confirmed)?)
}

/// The financial advisor's findings for the last complete quarter.
#[tauri::command]
#[specta::specta]
fn financial_findings(core: State<'_, Core>) -> Answer<Vec<FindingDto>> {
    Ok(core.financial_findings()?)
}

/// "Explain this" on an account or an entry, citing the entries behind it.
#[tauri::command]
#[specta::specta]
fn explain(
    core: State<'_, Core>,
    target: ExplainTargetDto,
    confirmed: bool,
) -> Answer<ExplanationDto> {
    Ok(core.explain(&target, confirmed)?)
}

/// The obligations calendar for a year, from the pack.
#[tauri::command]
#[specta::specta]
fn obligations(core: State<'_, Core>, year: i32) -> Answer<Vec<ObligationDto>> {
    Ok(core.obligations(year)?)
}

/// Whether advisors can run, and what to do if not.
#[tauri::command]
#[specta::specta]
fn advisor_status(core: State<'_, Core>) -> AdvisorStatusDto {
    core.advisor_status()
}

/// The rule pack in force, with citations.
#[tauri::command]
#[specta::specta]
fn rule_pack(core: State<'_, Core>) -> RulePackDto {
    core.rule_pack()
}

/// A document rendered to PDF (`cs` or `en`), base64-encoded.
#[tauri::command]
#[specta::specta]
fn invoice_pdf(core: State<'_, Core>, id: i64, lang: String) -> Answer<DocumentPdfDto> {
    Ok(core.invoice_pdf(id, &lang)?)
}

/// An issued document as XML: `isdoc`, `ubl` (Peppol BIS 3.0) or `cii`.
#[tauri::command]
#[specta::specta]
fn invoice_xml(core: State<'_, Core>, id: i64, format: String) -> Answer<DocumentXmlDto> {
    Ok(core.invoice_xml(id, &format)?)
}

/// Reminders due on a date, drafted for the user to send.
#[tauri::command]
#[specta::specta]
fn dunning_queue(core: State<'_, Core>, as_of: String) -> Answer<Vec<DunningNoticeDto>> {
    Ok(core.dunning_queue(&as_of)?)
}

/// Recurring invoice templates and their next runs.
#[tauri::command]
#[specta::specta]
fn recurring_templates(core: State<'_, Core>) -> Answer<Vec<RecurringTemplateDto>> {
    Ok(core.recurring_templates()?)
}

/// What the invoice editor offers.
#[tauri::command]
#[specta::specta]
fn invoice_form(core: State<'_, Core>) -> Answer<InvoiceFormDto> {
    Ok(core.invoice_form()?)
}

/// Saves a draft typed in the editor; the core parses and checks it.
#[tauri::command]
#[specta::specta]
fn create_invoice_draft(core: State<'_, Core>, draft: InvoiceDraftDto) -> Answer<InvoiceDto> {
    Ok(core.create_invoice_draft(&draft)?)
}

/// Issues a draft: the next number, posted through the kernel.
#[tauri::command]
#[specta::specta]
fn issue_invoice(core: State<'_, Core>, id: i64, issue_date: String) -> Answer<InvoiceDto> {
    Ok(core.issue_invoice(id, &issue_date)?)
}

/// Deletes a draft.
#[tauri::command]
#[specta::specta]
fn delete_invoice_draft(core: State<'_, Core>, id: i64) -> Answer<()> {
    Ok(core.delete_invoice_draft(id)?)
}

/// Imports a statement file (base64): tied out and deduplicated first.
#[tauri::command]
#[specta::specta]
fn import_bank_statement(
    core: State<'_, Core>,
    file_name: String,
    content_base64: String,
) -> Answer<BankStatementDto> {
    Ok(core.import_bank_statement(&file_name, &content_base64)?)
}

/// What importing invoices from Pohoda or Fakturoid would do; changes nothing.
#[tauri::command]
#[specta::specta]
fn preview_invoice_import(
    core: State<'_, Core>,
    file_name: String,
    content_base64: String,
) -> Answer<ImportPreviewDto> {
    Ok(core.preview_invoice_import(&file_name, &content_base64)?)
}

/// Posts the new invoices in a Pohoda or Fakturoid export.
#[tauri::command]
#[specta::specta]
fn commit_invoice_import(
    core: State<'_, Core>,
    file_name: String,
    content_base64: String,
) -> Answer<ImportPreviewDto> {
    Ok(core.commit_invoice_import(&file_name, &content_base64)?)
}

/// What the purchase editor offers.
#[tauri::command]
#[specta::specta]
fn purchase_form(core: State<'_, Core>) -> Answer<PurchaseFormDto> {
    Ok(core.purchase_form()?)
}

/// Every received invoice, with what's paid.
#[tauri::command]
#[specta::specta]
fn purchases(core: State<'_, Core>) -> Answer<Vec<PurchaseDto>> {
    Ok(core.purchases()?)
}

/// Records a received invoice: checked, posted, the supplier kept.
#[tauri::command]
#[specta::specta]
fn record_purchase(core: State<'_, Core>, draft: PurchaseDraftDto) -> Answer<PurchaseDto> {
    Ok(core.record_purchase(&draft)?)
}

/// Whether the opt-in update check is on, and what it last found.
#[tauri::command]
#[specta::specta]
fn update_status(core: State<'_, Core>) -> UpdateStatusDto {
    core.update_status()
}

/// Turns the update check on or off.
#[tauri::command]
#[specta::specta]
fn set_update_check(core: State<'_, Core>, enabled: bool) -> UpdateStatusDto {
    core.set_update_check(enabled)
}

/// Checks the project's signed release manifest for a newer version.
#[tauri::command]
#[specta::specta]
fn check_for_update(core: State<'_, Core>) -> Answer<UpdateStatusDto> {
    Ok(core.check_for_update()?)
}

/// Everything in the books as one reproducible zip (base64). The zip isn't
/// encrypted, so real books ask for the passphrase again first.
#[tauri::command]
#[specta::specta]
fn export_books(
    session: State<'_, Session>,
    core: State<'_, Core>,
    passphrase: Option<String>,
) -> Answer<ExportDto> {
    if !core.is_demo() {
        session
            .gate
            .confirm_passphrase(passphrase.as_deref().unwrap_or_default())?;
    }
    Ok(core.export_books()?)
}

/// Accepts every line the matcher or a rule is certain about.
#[tauri::command]
#[specta::specta]
fn accept_certain_bank_lines(core: State<'_, Core>) -> Answer<BankStatementDto> {
    Ok(core.accept_certain_bank_lines()?)
}

/// Books a line as the user chose: invoices, or account rows (a split).
#[tauri::command]
#[specta::specta]
fn book_bank_line(
    core: State<'_, Core>,
    line: String,
    allocations: Vec<BankAllocationDto>,
) -> Answer<BankStatementDto> {
    Ok(core.book_bank_line(&line, &allocations)?)
}

/// Makes a rule from a line and books the line by it.
#[tauri::command]
#[specta::specta]
fn create_bank_rule(
    core: State<'_, Core>,
    line: String,
    rule: BankRuleInputDto,
) -> Answer<BankStatementDto> {
    Ok(core.create_bank_rule(&line, &rule)?)
}

/// Reference data loaded, where it came from, and whether fetching is on.
#[tauri::command]
#[specta::specta]
fn reference_data(core: State<'_, Core>) -> Answer<RefDataDto> {
    Ok(core.reference_data()?)
}

/// Imports a ČNB rates file (`cnb_fx`) or a repo-rate history (`cnb_repo`).
#[tauri::command]
#[specta::specta]
fn import_reference_data(
    core: State<'_, Core>,
    kind: String,
    file_name: String,
    text: String,
) -> Answer<RefDataDto> {
    Ok(core.import_reference_data(&kind, &file_name, &text)?)
}

/// Turns fetching from the ČNB on or off (off by default).
#[tauri::command]
#[specta::specta]
fn set_reference_fetch(core: State<'_, Core>, enabled: bool) -> Answer<RefDataDto> {
    Ok(core.set_reference_fetch(enabled)?)
}

/// Fetches the ČNB's rates for a day, only when fetching is on.
#[tauri::command]
#[specta::specta]
fn fetch_cnb_rates(core: State<'_, Core>, date: String) -> Answer<RefDataDto> {
    Ok(core.fetch_cnb_rates(&date)?)
}

/// Checks a signed rule-pack update.
#[tauri::command]
#[specta::specta]
fn install_pack_update(
    core: State<'_, Core>,
    pack_toml: String,
    signature: String,
) -> Answer<PackUpdateDto> {
    Ok(core.install_pack_update(&pack_toml, &signature)?)
}

/// The session gate and what to do with books once they open: the shell
/// passes a callback that makes the core the app's managed state.
pub struct Session {
    gate: Gate,
    open: Box<dyn Fn(Core) + Send + Sync>,
    state: std::sync::Mutex<Option<SessionStateDto>>,
}

impl Session {
    /// A session over `gate` that hands opened books to `open`.
    pub fn new(gate: Gate, open: Box<dyn Fn(Core) + Send + Sync>) -> Self {
        Self {
            gate,
            open,
            state: std::sync::Mutex::new(None),
        }
    }

    /// Records books that were opened another way (the IPC test manages the
    /// demo core directly).
    pub fn mark_open(&self, state: SessionStateDto) {
        if let Ok(mut s) = self.state.lock() {
            *s = Some(state);
        }
    }

    /// Gets a freshly opened core ready (the CLI driver, the shim, a backup
    /// if one is due) and hands it over. Only the first is kept: books stay
    /// open until the app quits.
    pub fn adopt(&self, core: Core) -> SessionStateDto {
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
        let state = core.session_state();
        if let Ok(mut s) = self.state.lock()
            && s.is_none()
        {
            *s = Some(state.clone());
            (self.open)(core);
        }
        state
    }
}

/// Whether books are open, and if not, whether to set up or unlock.
#[tauri::command]
#[specta::specta]
fn session_state(session: State<'_, Session>) -> SessionStateDto {
    match session.state.lock().ok().and_then(|s| s.clone()) {
        Some(open) => open,
        None => session.gate.state(),
    }
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
    if let Some(open) = session.state.lock().ok().and_then(|s| s.clone()) {
        return Ok(open);
    }
    Ok(session.adopt(Core::demo()?))
}

/// The backups there are, and the policy.
#[tauri::command]
#[specta::specta]
fn backups(core: State<'_, Core>) -> Answer<BackupsDto> {
    Ok(core.backups()?)
}

/// Backs up now.
#[tauri::command]
#[specta::specta]
fn backup_now(core: State<'_, Core>) -> Answer<BackupDto> {
    Ok(core.backup_now(&skyla_app::session::system_now())?)
}

/// Restores the newest backup into a scratch folder and checks it.
#[tauri::command]
#[specta::specta]
fn restore_drill(core: State<'_, Core>) -> Answer<DrillDto> {
    Ok(core.restore_drill()?)
}

/// Every command, for the invoke handler and the TypeScript export.
pub fn specta_builder<R: Runtime>() -> Builder<R> {
    Builder::<R>::new()
        .commands(collect_commands![
            session_state,
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
            purchases,
            record_purchase,
            set_update_check,
            check_for_update,
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
            let handle = app.handle().clone();
            let session = Session::new(
                Gate::new(dir, keystore),
                Box::new(move |core| {
                    handle.manage(core);
                }),
            );
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
