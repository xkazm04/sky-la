//! Books and documents shared by the exchange-format tests.

#![allow(dead_code)]

use rusqlite::Connection;
use skyla_invoicing::{
    Accounts, Customer, DocKind, Document, DraftInput, LineInput, Supplier, apply_schema,
    create_draft, define_series, draft_credit_note, get, issue, set_supplier,
};
use skyla_ledger::{ChartSpec, NewEntry, NewLine, SourceKind, open_period, post_entry};
use skyla_money::{Currency, Money};
use skyla_rules::Pack;

const CZ_CHART: &str = include_str!("../../../../rules/cz/chart.toml");
pub struct Books {
    pub conn: Connection,
    pub pack: Pack,
    pub accounts: Accounts,
}

pub fn books(vat_payer: bool) -> Books {
    let conn = Connection::open_in_memory().expect("db");
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .expect("pragma");
    skyla_ledger::apply_schema(&conn).expect("ledger schema");
    apply_schema(&conn).expect("invoicing schema");
    skyla_ledger::seed_chart(&conn, &ChartSpec::from_toml(CZ_CHART).expect("chart")).expect("seed");
    skyla_ledger::set_functional_currency(&conn, Currency::CZK).expect("ccy");
    open_period(&conn, "2026-01-01", "2026-12-31").expect("2026");
    for (series, kind, pattern) in [
        ("FV", DocKind::Invoice, "{YYYY}-{NNN}"),
        ("OD", DocKind::CreditNote, "OD{YY}{NNNN}"),
        ("ZF", DocKind::Advance, "ZF{YYYY}-{NN}"),
        ("DZ", DocKind::AdvanceTax, "DZ{YYYY}-{NN}"),
    ] {
        define_series(&conn, series, kind, pattern, series).expect("series");
    }
    set_supplier(
        &conn,
        &Supplier {
            name: "Jana Nováková".into(),
            ico: Some("92588034".into()),
            dic: vat_payer.then(|| "CZ92588034".into()),
            address: "Korunní 2569/108\n101 00 Praha 10".into(),
            iban: Some("CZ6508000000192000145399".into()),
            bic: Some("GIBACZPX".into()),
            email: Some("jana@example.cz".into()),
            vat_payer,
            registration: "Fyzická osoba zapsaná v živnostenském rejstříku".into(),
        },
    )
    .expect("supplier");
    Books {
        conn,
        pack: Pack::cz_2026().expect("pack"),
        accounts: Accounts::cz(),
    }
}

pub fn line(
    description: &str,
    quantity: &str,
    unit: &str,
    unit_price_minor: i64,
    vat: &str,
) -> LineInput {
    LineInput {
        description: description.into(),
        quantity: quantity.into(),
        unit: unit.into(),
        unit_price_minor,
        vat_code: vat.into(),
        account: None,
    }
}

pub fn draft(kind: DocKind, series: &str, lines: Vec<LineInput>) -> DraftInput {
    DraftInput {
        kind,
        series: series.into(),
        customer: Customer {
            name: "Studio Brno s.r.o.".into(),
            ico: Some("91341272".into()),
            dic: Some("CZ91341272".into()),
            address: Some("Masarykova 12, 602 00 Brno".into()),
        },
        due_date: Some("2026-09-29".into()),
        tax_point_date: None,
        note: "Děkuji za spolupráci & těším se na další <projekt>.".into(),
        lines,
        related_id: None,
        advances: Vec::new(),
    }
}

impl Books {
    /// Issues a draft on `date` and returns the issued document.
    pub fn issue(&self, input: &DraftInput, date: &str) -> Document {
        let id = create_draft(&self.conn, input).expect("draft");
        issue(&self.conn, &self.pack, &self.accounts, id, date, None).expect("issue");
        get(&self.conn, &self.pack, id).expect("get")
    }

    /// A Workshop invoice and the full credit note against it.
    pub fn credit_chain(&self) -> (Document, Document) {
        let invoice = self.issue(
            &draft(
                DocKind::Invoice,
                "FV",
                vec![line("Workshop", "1", "ks", 1_200_000, "OUT21")],
            ),
            "2026-09-10",
        );
        let credit = draft_credit_note(
            &self.conn,
            &self.pack,
            invoice.id,
            "OD",
            None,
            "Workshop zrušen",
        )
        .expect("credit");
        issue(
            &self.conn,
            &self.pack,
            &self.accounts,
            credit,
            "2026-09-20",
            None,
        )
        .expect("issue");
        let credit = get(&self.conn, &self.pack, credit).expect("get");
        (invoice, credit)
    }

    /// An advance invoice, the tax document on its receipt, and the final
    /// invoice that deducts it.
    pub fn advance_chain(&self) -> (Document, Document, Document) {
        let advance = self.issue(
            &draft(
                DocKind::Advance,
                "ZF",
                vec![line("Záloha na web", "1", "", 10_000_000, "OUT21")],
            ),
            "2026-08-01",
        );
        let receipt = skyla_ledger::create_draft(
            &self.conn,
            &NewEntry {
                date: "2026-08-05".into(),
                source_kind: SourceKind::Bank,
                source_ref: None,
                memo: "Záloha".into(),
                created_by: "user".into(),
                lines: vec![
                    NewLine::debit("221", Money::new(12_100_000, Currency::CZK)),
                    NewLine::credit("324", Money::new(12_100_000, Currency::CZK)).expect("ok"),
                ],
            },
        )
        .expect("receipt");
        post_entry(&self.conn, receipt, None).expect("post");
        let mut tax = draft(
            DocKind::AdvanceTax,
            "DZ",
            vec![line("Přijatá záloha", "1", "", 12_100_000, "OUT21")],
        );
        tax.related_id = Some(advance.id);
        tax.due_date = None;
        tax.tax_point_date = Some("2026-08-05".into());
        let tax = self.issue(&tax, "2026-08-06");
        let mut fin = draft(
            DocKind::Invoice,
            "FV",
            vec![line("Web", "1", "", 15_000_000, "OUT21")],
        );
        fin.advances = vec![tax.id];
        fin.due_date = Some("2026-09-30".into());
        let fin = self.issue(&fin, "2026-09-15");
        (advance, tax, fin)
    }
}
