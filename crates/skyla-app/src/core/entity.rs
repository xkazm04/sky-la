//! A real entity (WP-30): created from the setup form in its own encrypted
//! database, its profile kept there, opened with the data key from the
//! vault. The demo is the same core over a fixture instead.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension};
use serde_json::json;
use skyla_store::DataKey;
use skyla_store::backup::{self, BackupPolicy, Keyed};

use super::{Core, bank, egress, refdata, update};
use crate::dto::{BackupDto, BackupsDto, DrillDto, EntitySetupDto};
use crate::error::CoreError;

const CZ_CHART: &str = include_str!("../../../../rules/cz/chart.toml");

/// What a real entity needs beyond the books: where they live and the key.
pub(crate) struct RealEntity {
    pub(crate) key: DataKey,
    pub(crate) books: PathBuf,
    pub(crate) backups: PathBuf,
    pub(crate) policy: BackupPolicy,
}

fn store(e: skyla_store::StoreError) -> CoreError {
    CoreError::BadRequest(e.to_string())
}

/// The setup form, checked; every problem listed.
fn check(setup: &EntitySetupDto) -> Result<(), CoreError> {
    let mut problems = Vec::new();
    if setup.display_name.trim().is_empty() {
        problems.push("enter your name or the business's".to_owned());
    }
    if !["monthly", "quarterly", "none"].contains(&setup.vat_period.as_str()) {
        problems.push("pick monthly, quarterly or not a VAT payer".into());
    }
    if !setup.ico.trim().is_empty() && !skyla_invoicing::valid_ico(setup.ico.trim()) {
        problems.push(format!("IČO {:?} isn't valid", setup.ico.trim()));
    }
    if setup.vat_period != "none" && setup.dic.as_deref().is_none_or(|d| d.trim().is_empty()) {
        problems.push("a VAT payer needs a DIČ".into());
    }
    if setup.address.trim().is_empty() {
        problems.push("enter the address printed on invoices".into());
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(CoreError::BadRequest(problems.join("; ")))
    }
}

fn profile_json(setup: &EntitySetupDto, as_of: &str) -> serde_json::Value {
    json!({
        "entity": {
            "displayName": setup.display_name.trim(),
            "legalForm": "OSVČ",
            "vatPeriod": setup.vat_period,
            "functionalCurrency": "CZK",
            "asOf": as_of,
            "bankAccount": "221",
            "bankName": setup.bank_name.trim(),
            "flatRateGroup": setup.flat_rate_group,
        },
        "supplier": {
            "name": setup.display_name.trim(),
            "ico": Some(setup.ico.trim()).filter(|s| !s.is_empty()),
            "dic": setup.dic.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            "address": setup.address.trim(),
            "iban": setup.iban.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            "bic": null,
            "email": setup.email.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            "vatPayer": setup.vat_period != "none",
            "registration": setup.registration.trim(),
        },
    })
}

fn year_of(day: &str) -> &str {
    day.get(..4).unwrap_or("2026")
}

fn ensure_year_open(conn: &Connection, today: &str) -> Result<(), CoreError> {
    let y = year_of(today);
    let (from, to) = (format!("{y}-01-01"), format!("{y}-12-31"));
    let periods = skyla_ledger::list_periods(conn)?;
    if !periods
        .iter()
        .any(|p| p.starts_on.as_str() <= today && p.ends_on.as_str() >= today)
    {
        skyla_ledger::open_period(conn, &from, &to)?;
    }
    Ok(())
}

impl Core {
    /// Puts a core together around an open connection.
    fn assemble(
        conn: Connection,
        domain: crate::demo::Domain,
        scheduled: std::collections::HashMap<i64, String>,
        real: Option<RealEntity>,
        pack: skyla_rules::Pack,
    ) -> Result<Self, CoreError> {
        let currency = skyla_ledger::functional_currency(&conn)?;
        let accounts = skyla_ledger::list_accounts(&conn)?
            .into_iter()
            .map(|a| (a.code, (a.name_en, a.kind)))
            .collect();
        let provider: Box<dyn skyla_advisor::LlmProvider> = if real.is_some() {
            Box::new(skyla_advisor::ClaudeCodeCli::from_system())
        } else {
            Box::new(crate::demo::demo_provider())
        };
        Ok(Self {
            conn: Mutex::new(conn),
            domain,
            accounts,
            currency,
            pack,
            scheduled,
            refdata: Mutex::new(refdata::RefData::new(refdata::https_fetcher())),
            updates: Mutex::new(update::UpdateState::new(update::release_fetcher())),
            drafts_created: std::sync::atomic::AtomicU64::new(0),
            bank: Mutex::new(bank::BankState::default()),
            provider: Mutex::new(provider),
            advisor_inbox: Mutex::new(Vec::new()),
            egress_policies: Mutex::new(egress::Policies::new()),
            shim: Mutex::new(PathBuf::from("skyla-mcp")),
            real,
        })
    }

    /// The demo core's parts (used by [`Core::demo`]).
    pub(crate) fn assemble_demo(
        conn: Connection,
        domain: crate::demo::Domain,
        scheduled: std::collections::HashMap<i64, String>,
    ) -> Result<Self, CoreError> {
        super::persist::apply_schema(&conn)?;
        Self::assemble(conn, domain, scheduled, None, skyla_rules::Pack::cz_2026()?)
    }

    /// Creates a new entity's books at `db_path`, encrypted with `key`.
    pub fn create_entity(
        db_path: &Path,
        key: &DataKey,
        setup: &EntitySetupDto,
        today: &str,
    ) -> Result<Self, CoreError> {
        check(setup)?;
        let conn = skyla_store::open_keyed(db_path, key, true).map_err(store)?;
        skyla_ledger::apply_schema(&conn)?;
        skyla_ledger::seed_chart(&conn, &skyla_ledger::ChartSpec::from_toml(CZ_CHART)?)?;
        skyla_ledger::set_functional_currency(&conn, skyla_money::Currency::CZK)?;
        ensure_year_open(&conn, today)?;
        let profile = profile_json(setup, today);
        conn.execute_batch(
            "CREATE TABLE app_profile (id INTEGER PRIMARY KEY CHECK (id = 1), json TEXT NOT NULL) STRICT;",
        )
        .map_err(skyla_ledger::LedgerError::from)?;
        conn.execute(
            "INSERT INTO app_profile (id, json) VALUES (1, ?1)",
            [profile.to_string()],
        )
        .map_err(skyla_ledger::LedgerError::from)?;
        let domain: crate::demo::Domain =
            serde_json::from_value(profile).map_err(|e| CoreError::BadRequest(e.to_string()))?;
        let pack = skyla_rules::Pack::cz_2026()?;
        let scheduled = crate::demo::seed_invoicing(&conn, &pack, &domain)?;
        skyla_egress::register::apply_schema(&conn)
            .map_err(|e| CoreError::BadRequest(e.to_string()))?;
        super::persist::apply_schema(&conn)?;
        let backups = db_path
            .parent()
            .map_or_else(|| PathBuf::from("Backups"), |d| d.join("Backups"));
        Self::assemble(
            conn,
            domain,
            scheduled,
            Some(RealEntity {
                key: key.clone(),
                books: db_path.to_path_buf(),
                backups,
                policy: BackupPolicy::default(),
            }),
            pack,
        )
    }

    /// Opens an entity's books at `db_path` with `key`; `today` is the date
    /// the books are seen on.
    pub fn open_entity(db_path: &Path, key: &DataKey, today: &str) -> Result<Self, CoreError> {
        Self::open_entity_trusting(db_path, key, today, refdata::TRUSTED_PACK_KEYS)
    }

    /// [`Core::open_entity`], trusting `pack_keys` for a saved pack update
    /// (tests: the shipped list is empty).
    #[doc(hidden)]
    pub fn open_entity_trusting(
        db_path: &Path,
        key: &DataKey,
        today: &str,
        pack_keys: &[&str],
    ) -> Result<Self, CoreError> {
        let conn = skyla_store::open_keyed(db_path, key, false).map_err(store)?;
        // Books from before saved state existed get the table now.
        super::persist::apply_schema(&conn)?;
        let pack = super::persist::pack_for(&conn, pack_keys)?;
        let json: Option<String> = conn
            .query_row("SELECT json FROM app_profile WHERE id = 1", [], |r| {
                r.get(0)
            })
            .optional()
            .map_err(skyla_ledger::LedgerError::from)?;
        let mut domain: crate::demo::Domain = serde_json::from_str(
            &json.ok_or_else(|| CoreError::BadRequest("these books have no profile".into()))?,
        )
        .map_err(|e| CoreError::BadRequest(e.to_string()))?;
        domain.entity.as_of = today.to_owned();
        ensure_year_open(&conn, today)?;
        let backups = db_path
            .parent()
            .map_or_else(|| PathBuf::from("Backups"), |d| d.join("Backups"));
        let core = Self::assemble(
            conn,
            domain,
            std::collections::HashMap::new(),
            Some(RealEntity {
                key: key.clone(),
                books: db_path.to_path_buf(),
                backups,
                policy: BackupPolicy::default(),
            }),
            pack,
        )?;
        core.restore_state()?;
        Ok(core)
    }

    /// The business details as set up, for Settings to edit.
    pub fn profile(&self) -> EntitySetupDto {
        let (e, s) = (&self.domain.entity, &self.domain.supplier);
        EntitySetupDto {
            display_name: e.display_name.clone(),
            ico: s.ico.clone().unwrap_or_default(),
            dic: s.dic.clone(),
            address: s.address.clone(),
            vat_period: e.vat_period.clone(),
            registration: s.registration.clone(),
            iban: s.iban.clone(),
            bank_name: e.bank_name.clone(),
            email: s.email.clone(),
            flat_rate_group: e.flat_rate_group.clone(),
        }
    }

    /// Changes the business details of real books: checked like the setup
    /// form, kept in the books, and printed on invoices issued from now on
    /// (issued ones keep what they were issued with). The VAT status isn't
    /// changed here: it follows the registration and changes the returns.
    /// Takes effect when the books are reopened ([`Core::reopen`]).
    pub fn update_profile(&self, setup: &EntitySetupDto) -> Result<(), CoreError> {
        if self.is_demo() {
            return Err(CoreError::BadRequest(
                "the demo's business details are fixed".into(),
            ));
        }
        check(setup)?;
        if setup.vat_period != self.domain.entity.vat_period {
            return Err(CoreError::BadRequest(
                "the VAT status follows your registration and changes how the returns are kept; it can't be changed here".into(),
            ));
        }
        let profile = profile_json(setup, &self.domain.entity.as_of);
        let supplier: crate::demo::DomainSupplier =
            serde_json::from_value(profile["supplier"].clone())
                .map_err(|e| CoreError::BadRequest(e.to_string()))?;
        let db = self.db();
        db.execute_batch("SAVEPOINT profile")
            .map_err(skyla_ledger::LedgerError::from)?;
        let result = (|| -> Result<(), CoreError> {
            db.execute(
                "UPDATE app_profile SET json = json_set(json, '$.entity', json(?1), '$.supplier', json(?2)) WHERE id = 1",
                [profile["entity"].to_string(), profile["supplier"].to_string()],
            )
            .map_err(skyla_ledger::LedgerError::from)?;
            skyla_invoicing::set_supplier(
                &db,
                &skyla_invoicing::Supplier {
                    name: supplier.name,
                    ico: supplier.ico,
                    dic: supplier.dic,
                    address: supplier.address,
                    iban: supplier.iban,
                    bic: supplier.bic,
                    email: supplier.email,
                    vat_payer: supplier.vat_payer,
                    registration: supplier.registration,
                },
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => db
                .execute_batch("RELEASE profile")
                .map_err(|e| CoreError::Ledger(skyla_ledger::LedgerError::from(e))),
            Err(e) => {
                let _ = db.execute_batch("ROLLBACK TO profile; RELEASE profile");
                Err(e)
            }
        }
    }

    /// The same books opened afresh, with what's stored now (after
    /// [`Core::update_profile`]).
    pub fn reopen(&self) -> Result<Self, CoreError> {
        let real = self.real()?;
        Self::open_entity(&real.books, &real.key, &self.domain.entity.as_of)
    }

    /// The demo, rather than real books.
    pub fn is_demo(&self) -> bool {
        self.real.is_none()
    }

    /// The session as seen once books are open.
    pub fn session_state(&self) -> crate::dto::SessionStateDto {
        crate::dto::SessionStateDto {
            state: if self.is_demo() { "demo" } else { "open" }.into(),
            entity: Some(self.domain.entity.display_name.clone()),
            remembered: false,
        }
    }

    fn real(&self) -> Result<&RealEntity, CoreError> {
        self.real.as_ref().ok_or_else(|| {
            CoreError::BadRequest("the demo keeps its books in memory and has no backups".into())
        })
    }

    fn chain_head(&self) -> Option<String> {
        skyla_ledger::verify_chain(&self.db())
            .ok()
            .and_then(|c| c.head)
    }

    /// Backs up now (`now`: UTC, RFC 3339) and prunes old backups.
    pub fn backup_now(&self, now: &str) -> Result<BackupDto, CoreError> {
        let real = self.real()?;
        let head = self.chain_head();
        let b = {
            let db = self.db();
            backup::backup_now(
                &Keyed {
                    conn: &db,
                    key: &real.key,
                },
                &real.backups,
                &self.domain.entity.display_name,
                now,
                head,
            )
            .map_err(store)?
        };
        backup::prune(
            &real.backups,
            &self.domain.entity.display_name,
            real.policy.keep,
        )
        .map_err(store)?;
        Ok(backup_dto(&b))
    }

    /// Backs up if the policy says one is due.
    pub fn backup_if_due(&self, now: &str) -> Result<Option<BackupDto>, CoreError> {
        let real = self.real()?;
        let list = backup::list(&real.backups, &self.domain.entity.display_name).map_err(store)?;
        if backup::is_due(real.policy, list.first(), now) {
            self.backup_now(now).map(Some)
        } else {
            Ok(None)
        }
    }

    /// The backups there are, and the policy.
    pub fn backups(&self) -> Result<BackupsDto, CoreError> {
        let Some(real) = self.real.as_ref() else {
            return Ok(BackupsDto {
                demo: true,
                folder: None,
                every_days: 0,
                keep: 0,
                backups: Vec::new(),
            });
        };
        let list = backup::list(&real.backups, &self.domain.entity.display_name).map_err(store)?;
        Ok(BackupsDto {
            demo: false,
            folder: Some(real.backups.display().to_string()),
            every_days: real.policy.every_days,
            keep: u32::try_from(real.policy.keep).unwrap_or(u32::MAX),
            backups: list.iter().map(backup_dto).collect(),
        })
    }

    /// Restores the newest backup into a scratch folder and checks it.
    pub fn restore_drill(&self) -> Result<DrillDto, CoreError> {
        let real = self.real()?;
        let list = backup::list(&real.backups, &self.domain.entity.display_name).map_err(store)?;
        let newest = list
            .first()
            .ok_or_else(|| CoreError::BadRequest("there's no backup to check yet".into()))?;
        // Beside the books, in the user's own folder: a shared temp folder
        // could be pre-created or swapped by another account to fake a pass.
        let scratch = real
            .backups
            .parent()
            .unwrap_or(&real.backups)
            .join(".restore-drill");
        let r = backup::drill(newest, &real.key, &scratch, |conn| {
            skyla_ledger::verify_chain(conn).ok().and_then(|c| c.head)
        })
        .map_err(store)?;
        Ok(DrillDto {
            file: r.file.clone(),
            passed: r.passed(),
            file_matches: r.file_matches,
            opens: r.opens,
            content_matches: r.content_matches,
            chain_matches: r.chain_matches,
        })
    }
}

fn backup_dto(b: &backup::Backup) -> BackupDto {
    BackupDto {
        file: b.file.display().to_string(),
        created_at: b.manifest.created_at.clone(),
        bytes: u32::try_from(b.manifest.bytes).unwrap_or(u32::MAX),
        chain_head: b.manifest.chain_head.clone(),
    }
}
