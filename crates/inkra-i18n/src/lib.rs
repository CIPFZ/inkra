//! # inkra-i18n
//!
//! **Layer L0 (cross-cutting)** (see SPEC 2.2). May be depended on by any layer and
//! depends on no internal crate.
//!
//! Loads the Fluent (`.ftl`) resources that are embedded in the binary and resolves
//! message ids to localized strings (NF-007). The UI language follows the system
//! locale unless the user picks one explicitly (PR-018).
//!
//! Lookup never fails: a missing message falls back to `en-US`, and if it is missing
//! there too the message id itself is returned and a warning is logged.

use std::borrow::Cow;
use std::sync::Arc;

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;

/// A language shipped with Inkra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    /// BCP-47 tag, e.g. `zh-CN`.
    pub tag: &'static str,
    /// Native display name, e.g. `简体中文`.
    pub native_name: &'static str,
    /// Raw `.ftl` source.
    source: &'static str,
}

/// The fallback language, used when a message is missing in the active language.
pub const FALLBACK: &str = "en-US";

/// All languages bundled with the application. `en-US` must be first.
pub const LANGUAGES: &[Language] = &[
    Language {
        tag: "en-US",
        native_name: "English",
        source: include_str!("../locales/en-US/inkra.ftl"),
    },
    Language {
        tag: "zh-CN",
        native_name: "简体中文",
        source: include_str!("../locales/zh-CN/inkra.ftl"),
    },
];

/// Localizer for one active language with `en-US` fallback.
///
/// Cheap to clone (shares bundles through `Arc`) and `Send + Sync`.
#[derive(Clone)]
pub struct I18n {
    lang: &'static str,
    active: Arc<FluentBundle<FluentResource>>,
    fallback: Arc<FluentBundle<FluentResource>>,
}

impl std::fmt::Debug for I18n {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("I18n").field("lang", &self.lang).finish()
    }
}

fn build_bundle(lang: &Language) -> FluentBundle<FluentResource> {
    let id: LanguageIdentifier = lang.tag.parse().unwrap_or_default();
    let mut bundle = FluentBundle::new_concurrent(vec![id]);
    // egui renders the Unicode isolation marks as tofu boxes; we never mix
    // directions inside a single label, so isolation is not needed.
    bundle.set_use_isolating(false);
    let resource = match FluentResource::try_new(lang.source.to_owned()) {
        Ok(r) => r,
        Err((r, errors)) => {
            log::warn!("i18n: {} has {} syntax errors", lang.tag, errors.len());
            r
        }
    };
    if let Err(errors) = bundle.add_resource(resource) {
        log::warn!("i18n: {} has {} resource errors", lang.tag, errors.len());
    }
    bundle
}

impl I18n {
    /// Creates a localizer for `tag`. Unknown tags are matched by primary language
    /// (`zh-TW` → `zh-CN`, `en-GB` → `en-US`) and finally fall back to `en-US`.
    pub fn new(tag: &str) -> Self {
        let lang = match_language(tag);
        let fallback = Arc::new(build_bundle(&LANGUAGES[0]));
        let active = if lang.tag == FALLBACK {
            Arc::clone(&fallback)
        } else {
            Arc::new(build_bundle(lang))
        };
        Self {
            lang: lang.tag,
            active,
            fallback,
        }
    }

    /// Creates a localizer that follows the operating system language.
    pub fn system() -> Self {
        Self::new(&system_language())
    }

    /// Tag of the active language.
    pub fn language(&self) -> &'static str {
        self.lang
    }

    /// Whether a message with `id` exists in the active or fallback bundle.
    pub fn has(&self, id: &str) -> bool {
        self.active.has_message(id) || self.fallback.has_message(id)
    }

    /// Resolves `id` without arguments.
    pub fn tr(&self, id: &str) -> String {
        self.format(id, None)
    }

    /// Resolves `id` with named arguments.
    pub fn tr_args(&self, id: &str, args: &[(&str, FluentValue<'_>)]) -> String {
        let mut fa = FluentArgs::new();
        for (k, v) in args {
            fa.set(*k, v.clone());
        }
        self.format(id, Some(&fa))
    }

    fn format(&self, id: &str, args: Option<&FluentArgs<'_>>) -> String {
        for bundle in [&self.active, &self.fallback] {
            if let Some(pattern) = bundle.get_message(id).and_then(|m| m.value()) {
                let mut errors = Vec::new();
                let s: Cow<'_, str> = bundle.format_pattern(pattern, args, &mut errors);
                if !errors.is_empty() {
                    log::warn!("i18n: errors formatting `{id}`: {errors:?}");
                }
                return s.into_owned();
            }
        }
        log::warn!("i18n: missing message `{id}`");
        id.to_owned()
    }
}

impl Default for I18n {
    fn default() -> Self {
        Self::new(FALLBACK)
    }
}

/// Picks the bundled language that best matches `tag`.
pub fn match_language(tag: &str) -> &'static Language {
    let norm = tag.replace('_', "-");
    let norm = norm.split('.').next().unwrap_or_default();
    if let Some(l) = LANGUAGES.iter().find(|l| l.tag.eq_ignore_ascii_case(norm)) {
        return l;
    }
    let primary = norm.split('-').next().unwrap_or_default();
    LANGUAGES
        .iter()
        .find(|l| {
            l.tag
                .split('-')
                .next()
                .is_some_and(|p| p.eq_ignore_ascii_case(primary))
        })
        .unwrap_or(&LANGUAGES[0])
}

/// The operating system UI language as a BCP-47 tag (defaults to `en-US`).
pub fn system_language() -> String {
    sys_locale::get_locale().unwrap_or_else(|| FALLBACK.to_owned())
}

/// Message ids defined in the raw `.ftl` source of `lang` (messages only, no terms).
pub fn message_ids(lang: &Language) -> Vec<&'static str> {
    lang.source
        .lines()
        .filter_map(|line| {
            let (id, _) = line.split_once('=')?;
            let id = id.trim_end();
            let first = id.chars().next()?;
            (first.is_ascii_alphabetic()
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .then_some(id)
        })
        .collect()
}

pub use fluent_bundle::FluentValue as Value;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_have_identical_message_sets() {
        let base: std::collections::BTreeSet<_> = message_ids(&LANGUAGES[0]).into_iter().collect();
        for lang in &LANGUAGES[1..] {
            let ids: std::collections::BTreeSet<_> = message_ids(lang).into_iter().collect();
            let missing: Vec<_> = base.difference(&ids).collect();
            let extra: Vec<_> = ids.difference(&base).collect();
            assert!(missing.is_empty(), "{} missing: {missing:?}", lang.tag);
            assert!(extra.is_empty(), "{} extra: {extra:?}", lang.tag);
        }
    }

    #[test]
    fn resources_parse_without_errors() {
        for lang in LANGUAGES {
            assert!(
                FluentResource::try_new(lang.source.to_owned()).is_ok(),
                "{}",
                lang.tag
            );
        }
    }

    #[test]
    fn lookup_and_fallback() {
        let zh = I18n::new("zh-CN");
        assert_eq!(zh.tr("app-name"), "Inkra");
        assert_eq!(zh.tr("menu-file"), "文件");
        assert_eq!(I18n::new("en-US").tr("menu-file"), "File");
        assert_eq!(zh.tr("no-such-message"), "no-such-message");
    }

    #[test]
    fn matching() {
        assert_eq!(match_language("zh_TW.UTF-8").tag, "zh-CN");
        assert_eq!(match_language("en-GB").tag, "en-US");
        assert_eq!(match_language("fr-FR").tag, "en-US");
        assert_eq!(match_language("ZH-cn").tag, "zh-CN");
    }

    #[test]
    fn arguments() {
        let en = I18n::new("en-US");
        let s = en.tr_args("about-version", &[("version", "1.2.3".into())]);
        assert_eq!(s, "Version 1.2.3");
    }
}
