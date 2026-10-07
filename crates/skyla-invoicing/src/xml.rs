//! A small, strict XML writer for the e-invoice formats: escaped text and
//! attributes, two-space indentation, elements closed in order.

use skyla_money::Money;

pub(crate) struct Xml {
    out: String,
    open: Vec<String>,
}

impl Xml {
    pub(crate) fn new() -> Self {
        Self {
            out: String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"),
            open: Vec::new(),
        }
    }

    fn indent(&mut self) {
        for _ in 0..self.open.len() {
            self.out.push_str("  ");
        }
    }

    fn start(&mut self, name: &str, attrs: &[(&str, &str)]) {
        self.indent();
        self.out.push('<');
        self.out.push_str(name);
        for (k, v) in attrs {
            self.out.push(' ');
            self.out.push_str(k);
            self.out.push_str("=\"");
            escape_into(&mut self.out, v, true);
            self.out.push('"');
        }
    }

    /// Opens an element with attributes; children follow until [`Self::close`].
    pub(crate) fn open_with(&mut self, name: &str, attrs: &[(&str, &str)]) {
        self.start(name, attrs);
        self.out.push_str(">\n");
        self.open.push(name.to_owned());
    }

    /// Opens an element.
    pub(crate) fn open(&mut self, name: &str) {
        self.open_with(name, &[]);
    }

    /// Closes the innermost open element.
    pub(crate) fn close(&mut self) {
        if let Some(name) = self.open.pop() {
            self.indent();
            self.out.push_str("</");
            self.out.push_str(&name);
            self.out.push_str(">\n");
        }
    }

    /// A text-only element with attributes.
    pub(crate) fn leaf_with(&mut self, name: &str, attrs: &[(&str, &str)], text: &str) {
        self.start(name, attrs);
        if text.is_empty() {
            self.out.push_str("/>\n");
            return;
        }
        self.out.push('>');
        escape_into(&mut self.out, text, false);
        self.out.push_str("</");
        self.out.push_str(name);
        self.out.push_str(">\n");
    }

    /// A text-only element.
    pub(crate) fn leaf(&mut self, name: &str, text: &str) {
        self.leaf_with(name, &[], text);
    }

    /// A text-only element, left out when the text is empty (Peppol forbids
    /// empty elements).
    pub(crate) fn leaf_opt(&mut self, name: &str, text: &str) {
        if !text.trim().is_empty() {
            self.leaf_with(name, &[], text.trim());
        }
    }

    /// An amount with its `currencyID`.
    pub(crate) fn amount(&mut self, name: &str, amount: Money) {
        let code = amount.currency().code().to_owned();
        self.leaf_with(name, &[("currencyID", &code)], &decimal(amount));
    }

    /// The document, every element closed.
    pub(crate) fn finish(mut self) -> String {
        while !self.open.is_empty() {
            self.close();
        }
        self.out
    }
}

fn escape_into(out: &mut String, text: &str, attribute: bool) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            '\n' if attribute => out.push_str("&#10;"),
            // XML 1.0 forbids most control characters outright.
            c if c.is_control() && !matches!(c, '\n' | '\t' | '\r') => out.push(' '),
            c => out.push(c),
        }
    }
}

/// `-1234.50` for -123 450 minor units of a two-decimal currency.
pub(crate) fn decimal(amount: Money) -> String {
    let units = u32::from(amount.currency().minor_units());
    let minor = amount.minor();
    if units == 0 {
        return minor.to_string();
    }
    let per = 10_u64.pow(units);
    let abs = minor.unsigned_abs();
    format!(
        "{}{}.{:0width$}",
        if minor < 0 { "-" } else { "" },
        abs / per,
        abs % per,
        width = units as usize
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use skyla_money::Currency;

    #[test]
    fn escapes_and_nests() {
        let mut x = Xml::new();
        x.open_with("a", &[("b", "x\"<&>")]);
        x.leaf("c", "Novák & syn <s.r.o.>");
        x.leaf("d", "");
        let s = x.finish();
        assert!(s.contains(r#"<a b="x&quot;&lt;&amp;&gt;">"#), "{s}");
        assert!(s.contains("<c>Novák &amp; syn &lt;s.r.o.&gt;</c>"), "{s}");
        assert!(s.contains("<d/>"));
        assert!(s.ends_with("</a>\n"));
    }

    #[test]
    fn writes_signed_decimals() {
        let czk = |m| Money::new(m, Currency::CZK);
        assert_eq!(decimal(czk(48_050)), "480.50");
        assert_eq!(decimal(czk(-1_250)), "-12.50");
        assert_eq!(decimal(czk(-5)), "-0.05");
        assert_eq!(decimal(czk(0)), "0.00");
        let jpy = Currency::from_code("JPY").expect("jpy");
        assert_eq!(decimal(Money::new(-120, jpy)), "-120");
    }
}
