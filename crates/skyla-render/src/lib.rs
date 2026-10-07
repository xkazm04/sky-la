//! Typst templates rendered to PDF.
//!
//! WP-12: invoices, credit notes and advance documents in Czech or English,
//! with the QR Platba code when there's something to pay. Rust prepares
//! every string the template prints (amounts, dates, the SPAYD payload);
//! the template only lays them out, so the PDF can't disagree with the
//! books. Everything the compiler reads lives in memory: the template, the
//! data, the QR image and the embedded Geist fonts. No file system, no
//! network, no clock, so the same document renders to the same bytes.

mod view;
mod world;

use qrcode::{EcLevel, QrCode};
use skyla_invoicing::Document;
use skyla_money::Money;
use typst::layout::{Frame, FrameItem, Point};
use typst_layout::PagedDocument;
use typst_pdf::PdfOptions;

pub use view::{InvoiceView, Lang, invoice_view};

/// What went wrong while rendering.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// The payment details can't form a QR Platba code.
    #[error(transparent)]
    Invoicing(#[from] skyla_invoicing::InvoicingError),
    /// The template failed to compile; a bug, never the user's input.
    #[error("the template failed: {0}")]
    Template(String),
    /// The QR payload doesn't fit a QR code.
    #[error("QR code: {0}")]
    Qr(String),
}

/// A rendered document.
#[derive(Debug, Clone)]
pub struct Rendered {
    /// The PDF bytes.
    pub pdf: Vec<u8>,
    /// The text layer, one string per page: lines top to bottom, runs on a
    /// line left to right, separated by two spaces where there is a gap.
    pub text: Vec<String>,
    /// The SPAYD string in the QR code, if the document has one.
    pub spayd: Option<String>,
}

const INVOICE_TEMPLATE: &str = include_str!("../templates/invoice.typ");

/// Renders an issued (or draft) document; see [`invoice_view`] for `due`
/// and `related`.
pub fn invoice_pdf(
    doc: &Document,
    due: Money,
    related: Option<&str>,
    lang: Lang,
) -> Result<Rendered, RenderError> {
    let view = invoice_view(doc, due, related, lang)?;
    // Issued numbers are unique and stable; drafts fall back to their uid.
    let ident = doc.number.as_deref().unwrap_or(&doc.uid);
    render_view(&view, ident)
}

/// Renders a prepared view. `ident` keeps the PDF's document id stable.
pub fn render_view(view: &InvoiceView, ident: &str) -> Result<Rendered, RenderError> {
    let data = serde_json::to_vec(view).map_err(|e| RenderError::Template(e.to_string()))?;
    let mut files = vec![("/data.json", data)];
    if let Some(spayd) = &view.spayd {
        files.push(("/qr.svg", qr_svg(spayd)?.into_bytes()));
    }
    let world = world::MemoryWorld::new(INVOICE_TEMPLATE, files);
    let compiled = typst::compile::<PagedDocument>(&world);
    let document = compiled.output.map_err(|errors| {
        RenderError::Template(
            errors
                .iter()
                .map(|e| e.message.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    let options = PdfOptions {
        ident: typst::foundations::Smart::Custom(ident.to_owned()),
        creator: typst::foundations::Smart::Custom(Some("sky-la".to_owned())),
        timestamp: None,
        ..PdfOptions::default()
    };
    let pdf = typst_pdf::pdf(&document, &options).map_err(|errors| {
        RenderError::Template(
            errors
                .iter()
                .map(|e| e.message.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    let text = document
        .pages()
        .iter()
        .map(|p| text_layer(&p.frame))
        .collect();
    Ok(Rendered {
        pdf,
        text,
        spayd: view.spayd.clone(),
    })
}

/// The QR code for `payload` (error correction M, as QR Platba asks).
pub fn qr_code(payload: &str) -> Result<QrCode, RenderError> {
    QrCode::with_error_correction_level(payload.as_bytes(), EcLevel::M)
        .map_err(|e| RenderError::Qr(e.to_string()))
}

/// The QR code as a crisp SVG: one path, a four-module quiet zone.
pub fn qr_svg(payload: &str) -> Result<String, RenderError> {
    let code = qr_code(payload)?;
    let width = code.width();
    let colors = code.to_colors();
    let quiet = 4;
    let size = width + 2 * quiet;
    let mut path = String::new();
    for y in 0..width {
        for x in 0..width {
            if colors[y * width + x] == qrcode::Color::Dark {
                path.push_str(&format!("M{} {}h1v1h-1z", x + quiet, y + quiet));
            }
        }
    }
    Ok(format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {size} {size}" shape-rendering="crispEdges"><rect width="{size}" height="{size}" fill="#fff"/><path fill="#000" d="{path}"/></svg>"##
    ))
}

struct Run {
    x: f64,
    y: f64,
    end: f64,
    text: String,
}

fn text_layer(frame: &Frame) -> String {
    let mut runs = Vec::new();
    collect(frame, Point::zero(), &mut runs);
    runs.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let mut lines: Vec<Vec<Run>> = Vec::new();
    for run in runs {
        match lines.last_mut() {
            Some(line) if (line[0].y - run.y).abs() < 2.0 => line.push(run),
            _ => lines.push(vec![run]),
        }
    }
    lines
        .into_iter()
        .map(|mut line| {
            line.sort_by(|a, b| a.x.total_cmp(&b.x));
            let mut out = String::new();
            let mut end = f64::NEG_INFINITY;
            for run in line {
                if !out.is_empty() {
                    out.push_str(if run.x - end > 3.0 { "  " } else { "" });
                }
                out.push_str(&run.text);
                end = run.end;
            }
            out.trim_end().to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn collect(frame: &Frame, offset: Point, runs: &mut Vec<Run>) {
    for (pos, item) in frame.items() {
        let at = offset + *pos;
        match item {
            FrameItem::Group(group) => {
                let shifted = at + Point::new(group.transform.tx, group.transform.ty);
                collect(&group.frame, shifted, runs);
            }
            FrameItem::Text(text) => {
                let x = at.x.to_pt();
                runs.push(Run {
                    x,
                    y: at.y.to_pt(),
                    end: x + text.width().to_pt(),
                    text: text.text.to_string(),
                });
            }
            _ => {}
        }
    }
}
