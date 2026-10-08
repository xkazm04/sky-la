//! Splits a free-text Czech postal address into the parts e-invoice formats
//! ask for: street, building number, postal code, city.

/// An address in parts. Anything that doesn't parse stays in `street`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AddressParts {
    /// Street (or the whole first line).
    pub street: String,
    /// Building number, e.g. `2569/108`.
    pub building: String,
    /// Postal code without spaces, e.g. `10100`.
    pub postal_zone: String,
    /// City, e.g. `Praha 10`.
    pub city: String,
    /// ISO 3166-1 alpha-2; `CZ` unless the address names another.
    pub country: String,
}

/// Like [`split_address`], for a party with a VAT number: when the address
/// doesn't name its country and the number carries another member state's
/// prefix (`DE123456789`), that state is the country.
pub fn split_address_for(text: &str, vat_id: Option<&str>) -> AddressParts {
    let mut parts = split_address(text);
    let named = text
        .split(['\n', ','])
        .map(str::trim)
        .rfind(|p| !p.is_empty())
        .is_some_and(|last| country_code(last).is_some());
    if !named
        && let Some(id) = vat_id.and_then(crate::supplier::eu_vat_id)
        && let Some(country) = crate::supplier::eu_country_of_prefix(&id.country)
    {
        parts.country = country.into();
    }
    parts
}

/// Splits `Korunní 2569/108\n101 00 Praha 10` (or the same with a comma)
/// into its parts.
pub fn split_address(text: &str) -> AddressParts {
    let parts: Vec<&str> = text
        .split(['\n', ','])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let mut out = AddressParts {
        country: "CZ".into(),
        ..AddressParts::default()
    };
    let mut rest = parts.as_slice();
    // A trailing country name or code.
    if let Some((last, before)) = rest.split_last()
        && let Some(code) = country_code(last)
    {
        out.country = code.into();
        rest = before;
    }
    // The postal line: five digits (optionally `NNN NN`) then the city.
    if let Some(i) = rest.iter().rposition(|p| postal(p).is_some())
        && let Some((zone, city)) = postal(rest[i])
    {
        out.postal_zone = zone;
        out.city = city;
        let mut lines = rest.to_vec();
        lines.remove(i);
        rest = &[];
        out.street = lines.join(", ");
    }
    if !rest.is_empty() {
        out.street = rest.join(", ");
    }
    // The building number is the street line's last token when it holds a digit.
    if let Some((street, number)) = out.street.rsplit_once(' ')
        && number.chars().next().is_some_and(|c| c.is_ascii_digit())
    {
        out.building = number.to_owned();
        out.street = street.trim_end().to_owned();
    }
    out
}

fn postal(line: &str) -> Option<(String, String)> {
    let chars: Vec<char> = line.chars().collect();
    let (digits, consumed) = match chars.as_slice() {
        [a, b, c, ' ', d, e, ..] => ([*a, *b, *c, *d, *e], 6),
        [a, b, c, d, e, ..] => ([*a, *b, *c, *d, *e], 5),
        _ => return None,
    };
    if !digits.iter().all(char::is_ascii_digit) {
        return None;
    }
    let city: String = chars[consumed..].iter().collect();
    let city = city.trim();
    if chars.get(consumed).is_some_and(|c| !c.is_whitespace()) || city.is_empty() {
        return None;
    }
    Some((digits.iter().collect(), city.to_owned()))
}

fn country_code(text: &str) -> Option<&'static str> {
    // Any member state by its two capitals: `FR`, `IT`.
    if text.len() == 2
        && let Some(code) = crate::supplier::eu_country_of_prefix(text)
    {
        return Some(code);
    }
    match text.to_lowercase().as_str() {
        "česká republika" | "czech republic" | "czechia" | "česko" | "cz" => Some("CZ"),
        "slovensko" | "slovakia" | "slovenská republika" | "sk" => Some("SK"),
        "německo" | "germany" | "deutschland" | "de" => Some("DE"),
        "rakousko" | "austria" | "österreich" | "at" => Some("AT"),
        "polsko" | "poland" | "polska" | "pl" => Some("PL"),
        _ => None,
    }
}

/// The country's name for documents, in Czech.
pub fn country_name_cs(code: &str) -> &'static str {
    match code {
        "CZ" => "Česká republika",
        "SK" => "Slovensko",
        "DE" => "Německo",
        "AT" => "Rakousko",
        "PL" => "Polsko",
        "FR" => "Francie",
        "IT" => "Itálie",
        "NL" => "Nizozemsko",
        "ES" => "Španělsko",
        "BE" => "Belgie",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_common_shapes() {
        let a = split_address("Korunní 2569/108\n101 00 Praha 10");
        assert_eq!(
            a,
            AddressParts {
                street: "Korunní".into(),
                building: "2569/108".into(),
                postal_zone: "10100".into(),
                city: "Praha 10".into(),
                country: "CZ".into(),
            }
        );
        let b = split_address("Vinohradská 1, 120 00 Praha 2");
        assert_eq!(
            (b.street.as_str(), b.building.as_str()),
            ("Vinohradská", "1")
        );
        assert_eq!(
            (b.postal_zone.as_str(), b.city.as_str()),
            ("12000", "Praha 2")
        );
        let c = split_address("Hlavná 5, 81101 Bratislava, Slovensko");
        assert_eq!((c.country.as_str(), c.city.as_str()), ("SK", "Bratislava"));
        let d = split_address("Na Příkopě 12");
        assert_eq!(
            (d.street.as_str(), d.building.as_str(), d.city.as_str()),
            ("Na Příkopě", "12", "")
        );
        let e = split_address("Dvůr Lhota");
        assert_eq!((e.street.as_str(), e.building.as_str()), ("Dvůr Lhota", ""));
    }

    #[test]
    fn a_foreign_vat_number_names_the_country_when_the_address_does_not() {
        let plain = "Rue de la Paix 4\n75002 Paris";
        assert_eq!(split_address(plain).country, "CZ");
        assert_eq!(
            split_address_for(plain, Some("FR12345678901")).country,
            "FR"
        );
        assert_eq!(split_address_for(plain, Some("CZ12345678")).country, "CZ");
        assert_eq!(split_address_for(plain, None).country, "CZ");
        // An address that names its country wins, by name or by code.
        let named = "Hlavná 5, 81101 Bratislava, Slovensko";
        assert_eq!(split_address_for(named, Some("DE123456789")).country, "SK");
        assert_eq!(split_address("Via Roma 1, 00100 Roma, IT").country, "IT");
        // Greece: the VAT prefix is EL, the ISO code GR.
        assert_eq!(split_address_for(plain, Some("EL123456789")).country, "GR");
    }
}
