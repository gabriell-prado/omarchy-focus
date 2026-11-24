//! Strings for the daemon's own output (notifications). It embeds the same
//! catalogs the QML plugin reads, so there is one set of translations, and
//! follows the language picked in Omarchy (~/.config/omarchy/language).

use serde_json::{Map, Value};
use std::fs;
use std::path::Path;

const CATALOGS: &[(&str, &str)] = &[
    ("en", include_str!("../../i18n/en.json")),
    ("pt-BR", include_str!("../../i18n/pt-BR.json")),
];

pub struct Strings {
    language: String,
    primary: Map<String, Value>,
    english: Map<String, Value>,
}

fn parse(raw: &str) -> Map<String, Value> {
    serde_json::from_str::<Value>(raw).ok().and_then(|v| v.as_object().cloned()).unwrap_or_default()
}

/// The catalog for `language`: exact match, then any catalog of the same base
/// language (pt → pt-BR), then English.
fn catalog_for(language: &str) -> &'static str {
    let base = language.split('-').next().unwrap_or("");
    CATALOGS
        .iter()
        .find(|(code, _)| *code == language)
        .or_else(|| CATALOGS.iter().find(|(code, _)| code.split('-').next() == Some(base)))
        .map(|(_, raw)| *raw)
        .unwrap_or(CATALOGS[0].1)
}

pub fn language_from_file(path: &Path) -> String {
    let code = fs::read_to_string(path).unwrap_or_default();
    let code = code.trim();
    if code.is_empty() {
        "en".into()
    } else {
        code.chars().take(16).collect()
    }
}

impl Strings {
    pub fn new(language: &str) -> Strings {
        Strings { language: language.to_string(), primary: parse(catalog_for(language)), english: parse(CATALOGS[0].1) }
    }

    fn lookup(&self, key: &str) -> Option<&Value> {
        self.primary.get(key).or_else(|| self.english.get(key))
    }

    pub fn t(&self, key: &str) -> String {
        self.lookup(key).and_then(Value::as_str).unwrap_or(key).to_string()
    }

    /// A plural entry ({"one": …, "other": …}) for `count`, with %1 filled in.
    pub fn tn(&self, key: &str, count: u32) -> String {
        let form = match self.lookup(key) {
            Some(Value::Object(forms)) => {
                let category = plural_category(&self.language, count);
                forms.get(category).or_else(|| forms.get("other")).and_then(Value::as_str).unwrap_or(key)
            }
            Some(Value::String(s)) => s.as_str(),
            _ => key,
        };
        form.replace("%1", &count.to_string())
    }
}

/// CLDR cardinal category for the languages that ship catalogs; anything else
/// uses the English rule.
fn plural_category(language: &str, n: u32) -> &'static str {
    let base = language.split('-').next().unwrap_or("");
    let one = match base {
        // Brazilian Portuguese counts 0 and 1 as "one"; European Portuguese only 1.
        "pt" if language != "pt-PT" => n <= 1,
        _ => n == 1,
    };
    if one {
        "one"
    } else {
        "other"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_and_pluralises() {
        let en = Strings::new("en");
        assert_eq!(en.t("notify.focusDone.title"), "Focus session complete");
        assert_eq!(en.tn("hero.today", 1), "1 session today");
        assert_eq!(en.tn("hero.today", 3), "3 sessions today");
        let pt = Strings::new("pt");
        assert_eq!(pt.t("notify.focusDone.title"), "Sessão de foco concluída");
        assert_eq!(pt.tn("notify.short.manual", 1), "Faça uma pausa de 1 minuto quando quiser.");
        assert_eq!(Strings::new("de").t("phase.focus"), "Focus", "unknown language uses English");
        assert_eq!(en.t("no.such.key"), "no.such.key");
    }
}
