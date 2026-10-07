// sky-la invoice template (cs/en). Every string arrives formatted from
// Rust in data.json; the template lays it out and does no arithmetic.

#let d = json("data.json")
#let cs = d.lang == "cs"

#let ink = rgb("#1d1d1f")
#let muted = rgb("#6e6e73")
#let hair = rgb("#d2d2d7")
#let accent = rgb("#0a5bc2")
#let wash = rgb("#f5f5f7")

#let L = if cs {
  (
    supplier: "Dodavatel",
    customer: "Odběratel",
    ico: "IČO",
    dic: "DIČ",
    issued: "Datum vystavení",
    taxpoint: "Datum zdanitelného plnění",
    due: "Datum splatnosti",
    description: "Popis",
    quantity: "Množství",
    unit-price: "Cena za j.",
    rate: "DPH",
    rate-head: "Sazba",
    amount: "Základ",
    amount-novat: "Částka",
    recap: "Rekapitulace DPH",
    base: "Základ",
    vat: "DPH",
    with-vat: "Celkem",
    total: "Celkem",
    advances: "Uhrazené zálohy",
    to-pay: "K úhradě",
    correction: "Výše opravy",
    bank: "Bankovní účet",
    vs: "Variabilní symbol",
    qr: "QR Platba",
    not-vat-payer: "Dodavatel není plátcem DPH.",
    reverse-charge: "Daň odvede zákazník (přenesená daňová povinnost).",
    corrects: "Opravuje doklad",
    covers: "K zálohové faktuře",
    draft: "KONCEPT – neplatný doklad",
    advance-note: "Zálohová faktura není daňovým dokladem.",
    footer: "Vystaveno v aplikaci sky-la",
    pack: "výpočet podle",
    page: "Strana",
  )
} else {
  (
    supplier: "Supplier",
    customer: "Customer",
    ico: "Company ID",
    dic: "VAT ID",
    issued: "Issue date",
    taxpoint: "Tax point",
    due: "Due date",
    description: "Description",
    quantity: "Quantity",
    unit-price: "Unit price",
    rate: "VAT",
    rate-head: "Rate",
    amount: "Net",
    amount-novat: "Amount",
    recap: "VAT summary",
    base: "Net",
    vat: "VAT",
    with-vat: "Gross",
    total: "Total",
    advances: "Advances paid",
    to-pay: "Amount due",
    correction: "Correction amount",
    bank: "Bank account",
    vs: "Payment reference",
    qr: "QR payment",
    not-vat-payer: "The supplier is not registered for VAT.",
    reverse-charge: "Reverse charge: VAT to be accounted for by the customer.",
    corrects: "Corrects document",
    covers: "For advance invoice",
    draft: "DRAFT – not a valid document",
    advance-note: "An advance invoice is not a tax document.",
    footer: "Issued with sky-la",
    pack: "computed with",
    page: "Page",
  )
}

#let title = if d.kind == "credit_note" {
  if cs { "Opravný daňový doklad" } else { "Credit note" }
} else if d.kind == "advance" {
  if cs { "Zálohová faktura" } else { "Advance invoice" }
} else if d.kind == "advance_tax" {
  if cs { "Daňový doklad k přijaté platbě" } else { "Tax document for a received payment" }
} else if d.vat_payer {
  if cs { "Faktura – daňový doklad" } else { "Tax invoice" }
} else {
  if cs { "Faktura" } else { "Invoice" }
}

#set document(title: title + " " + d.number, author: d.supplier.name)
#set text(font: "Geist", size: 9pt, fill: ink, lang: d.lang)
#set par(leading: 0.5em)
#set page(
  paper: "a4",
  margin: (x: 18mm, top: 18mm, bottom: 22mm),
  footer: context {
    set text(size: 7pt, fill: muted)
    grid(
      columns: (1fr, auto),
      [#L.footer#if d.pack != none [ · #L.pack #d.pack]],
      [#L.page #counter(page).display()],
    )
  },
  background: if not d.issued {
    rotate(-30deg, text(size: 44pt, weight: "semibold", fill: rgb(214, 48, 49, 40), L.draft))
  },
)

#let label(body) = text(size: 7.5pt, fill: muted, weight: "medium", body)

#let party(heading, p) = {
  label(heading)
  v(3pt)
  text(size: 10.5pt, weight: "semibold", p.name)
  linebreak()
  for line in p.address [#line \ ]
  if p.ico != none [#L.ico #p.ico \ ]
  if p.dic != none [#L.dic #p.dic \ ]
  if p.at("email", default: none) != none [#p.email \ ]
  if p.at("registration", default: none) != none {
    v(2pt)
    text(size: 7.5pt, fill: muted, p.registration)
  }
}

// Header: title and number.
#grid(
  columns: (1fr, auto),
  align: (left + bottom, right + bottom),
  text(size: 18pt, weight: "semibold", title),
  text(size: 14pt, weight: "medium", fill: accent, d.number),
)
#if d.related_number != none {
  v(2pt)
  text(fill: muted)[#if d.kind == "credit_note" { L.corrects } else { L.covers } #d.related_number]
}
#v(10pt)
#line(length: 100%, stroke: 0.5pt + hair)
#v(10pt)

// Parties.
#grid(
  columns: (1fr, 1fr),
  column-gutter: 14mm,
  party(L.supplier, d.supplier),
  party(L.customer, d.customer),
)
#v(12pt)

// Dates.
#{
  let dates = ()
  if d.issue_date != none { dates.push((L.issued, d.issue_date)) }
  if d.vat_payer and d.tax_point_date != none and d.kind != "advance" {
    dates.push((L.taxpoint, d.tax_point_date))
  }
  if d.due_date != none { dates.push((L.due, d.due_date)) }
  grid(
    columns: dates.len() * (auto,),
    column-gutter: 12mm,
    ..dates.map(((k, v)) => [#label(k) \ #text(weight: "medium", v)]),
  )
}
#v(14pt)

// Lines.
#{
  let vat-cols = d.vat_payer and d.kind != "advance"
  let head = if vat-cols {
    (L.description, L.quantity, L.unit-price, L.rate, L.amount)
  } else {
    (L.description, L.quantity, L.unit-price, L.amount-novat)
  }
  let cols = if vat-cols { (1fr, auto, auto, auto, auto) } else { (1fr, auto, auto, auto) }
  table(
    columns: cols,
    align: (x, _) => if x == 0 { left } else { right },
    stroke: (_, y) => if y == 0 { (bottom: 0.5pt + hair) } else { (bottom: 0.25pt + hair) },
    inset: (x, _) => (left: if x == 0 { 0pt } else { 8mm }, right: 0pt, y: 5pt),
    table.header(..head.map(h => label(h))),
    ..d.lines
      .map(l => if vat-cols {
        (l.description, l.quantity, l.unit_price, l.rate, l.amount)
      } else {
        (l.description, l.quantity, l.unit_price, l.amount)
      })
      .flatten(),
  )
}
#v(10pt)

// VAT recap and totals.
#grid(
  columns: (1fr, auto),
  column-gutter: 14mm,
  if d.vat_payer and d.kind != "advance" and d.recap.len() > 0 {
    label(L.recap)
    v(2pt)
    table(
      columns: (auto, auto, auto, auto),
      align: right,
      stroke: none,
      inset: (x: 0pt, y: 3pt),
      column-gutter: 7mm,
      label(L.rate-head), label(L.base), label(L.vat), label(L.with-vat),
      ..d.recap.map(r => (r.rate, r.base, r.vat, r.gross)).flatten(),
    )
  } else if not d.vat_payer {
    text(fill: muted, L.not-vat-payer)
  },
  {
    set align(right)
    let rows = ()
    if d.vat_payer and d.kind != "advance" {
      rows.push((L.base, d.base + " " + d.currency))
      rows.push((L.vat, d.vat + " " + d.currency))
    }
    rows.push((L.total, d.gross + " " + d.currency))
    if d.advances != none { rows.push((L.advances, "−" + d.advances + " " + d.currency)) }
    table(
      columns: (auto, auto),
      align: (left, right),
      stroke: none,
      inset: (x: 0pt, y: 3pt),
      column-gutter: 8mm,
      ..rows.map(((k, v)) => (text(fill: muted, k), v)).flatten(),
    )
    v(4pt)
    block(
      fill: wash,
      radius: 6pt,
      inset: (x: 10pt, y: 8pt),
      grid(
        columns: (auto, auto),
        column-gutter: 8mm,
        align: (left + horizon, right + horizon),
        text(weight: "medium", if d.kind == "credit_note" { L.correction } else { L.to-pay }),
        text(size: 14pt, weight: "semibold", d.due),
      ),
    )
  },
)

#if d.reverse_charge {
  v(8pt)
  text(weight: "medium", L.reverse-charge)
}
#if d.kind == "advance" {
  v(8pt)
  text(fill: muted, L.advance-note)
}
#if d.note != "" {
  v(10pt)
  d.note
}

// Payment.
#if d.payment != none and d.kind != "credit_note" and d.kind != "advance_tax" {
  v(18pt)
  line(length: 100%, stroke: 0.5pt + hair)
  v(10pt)
  grid(
    columns: (1fr, auto),
    column-gutter: 10mm,
    {
      label(L.bank)
      linebreak()
      text(size: 10.5pt, weight: "medium", d.payment.iban)
      if d.payment.bic != none [ · BIC #d.payment.bic]
      if d.payment.variable_symbol != none {
        v(6pt)
        label(L.vs)
        linebreak()
        text(size: 10.5pt, weight: "medium", d.payment.variable_symbol)
      }
    },
    if d.spayd != none {
      align(center)[
        #image("qr.svg", width: 30mm)
        #v(-2pt)
        #text(size: 7pt, fill: muted, L.qr)
      ]
    },
  )
}
