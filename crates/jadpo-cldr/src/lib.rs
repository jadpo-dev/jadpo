//! The Unicode CLDR data a deployable carries (area 10, "Locales"; roadmap
//! chunk 8.2c; docs/compiler-runtime.md §2.6, "The locale data a
//! deployable carries").
//!
//! `jadpo test` and `jadpo dev` run with every locale `Locale` lists, from
//! ICU4X's compiled data. A deployable needs only its application's
//! supported locales: [`export`] bakes, from that same compiled data
//! ([`Compiled`]), the data for them, which the deployable's build compiles
//! in place of the full set (ICU4X's `ICU4X_DATA_DIR`). What it keeps:
//!
//! - every marker the compiled data has, so each `compiled_data`
//!   constructor finds its own;
//! - of a marker keyed by locale, the entries a supported locale reaches
//!   as ICU4X looks data up: the locale itself and each one its fallback
//!   walks through for that marker, down to the root (`en-GB`, `en-001`,
//!   `en`, `und`), so each lookup in a supported locale finds the entry it
//!   finds with the full set, and writes the same text; and the root's,
//!   with none declared. Collation is kept the same way, since
//!   `list.sort_by` in a locale the app doesn't support orders by one it
//!   does (chunk 8.4i), and sorting by a locale at all needs `locales { }`.
//!   Case mapping takes any `Locale` (area 19,
//!   "Text"), and its data is one table;
//! - every marker with one entry for all locales in full (Unicode's
//!   properties, normalisation, likely subtags, the fallback's parents).
//!
//! Nothing is recomputed from CLDR's sources: each entry kept is the
//! compiled one, baked again.

// The baked data names `alloc::` and the components as `icu::`.
extern crate alloc;

mod markers;

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use icu_locale_core::LanguageIdentifier;
use icu_locale_fallback::{LocaleFallbacker, LocaleFallbackerBorrowed};
use icu_provider::export::{ExportMarker, ExportableProvider};
use icu_provider::prelude::*;
use icu_provider::{DynamicDataProvider, IterableDynamicDataProvider};
use icu_provider_export::baked_exporter::{BakedExporter, Options};
use icu_provider_export::{DataLocaleFamily, DeduplicationStrategy, ExportDriver};

/// The components by the names the baked data gives their types
/// (`icu::datetime::provider::...`), as the `icu` crate would.
mod icu {
    pub use icu_calendar as calendar;
    pub use icu_casemap as casemap;
    pub use icu_collator as collator;
    pub use icu_collections as collections;
    pub use icu_datetime as datetime;
    pub use icu_decimal as decimal;
    pub use icu_experimental as experimental;
    pub use icu_list as list;
    pub use icu_locale as locale;
    pub use icu_normalizer as normalizer;
    pub use icu_plurals as plurals;
    pub use icu_properties as properties;
    pub use icu_time as time;
}

/// The data compiled into this Jadpo, every marker of it: what an export
/// reads.
#[derive(Clone, Copy, Debug)]
pub struct Compiled;

/// [`Compiled`]'s markers, each implemented from its data crate's own
/// macro, with the entries it holds listed, so the exporter can walk them.
/// A marker keyed by locale lists them through its macro's `ITER` arm; one
/// with a single entry lists it here, since that arm names
/// `std::collections::BtreeSet` in this release and doesn't compile.
macro_rules! compiled {
    (
        $($data:ident :: $implement:ident => $marker:ty,)*
        ;
        $($single_data:ident :: $single_implement:ident => $single:ty,)*
    ) => {
        #[allow(unused_imports)]
        const _: () = {
            // Each `impl_` macro calls its crate's `__impl_` one by name.
            use icu_calendar_data::*;
            use icu_casemap_data::*;
            use icu_collator_data::*;
            use icu_datetime_data::*;
            use icu_decimal_data::*;
            use icu_experimental_data::*;
            use icu_list_data::*;
            use icu_locale_data::*;
            use icu_locale_fallback_data::*;
            use icu_normalizer_data::*;
            use icu_plurals_data::*;
            use icu_properties_data::*;
            use icu_time_data::*;
            icu_calendar_data::make_provider!(Compiled);
            $( $data::$implement!(Compiled, ITER); )*
            $( $single_data::$single_implement!(Compiled); )*
        };
        $(
            impl icu_provider::IterableDataProvider<$single> for Compiled {
                fn iter_ids(&self) -> Result<BTreeSet<DataIdentifierCow<'static>>, DataError> {
                    Ok([DataIdentifierCow::default()].into_iter().collect())
                }
            }
        )*
        icu_provider::export::make_exportable_provider!(
            Compiled,
            [$($marker,)* $($single,)*]
        );
    };
}

markers::each_marker!(compiled);

/// What a deployable for some locales carries of [`Compiled`]: every
/// marker, of each keyed by locale only the entries those locales reach.
#[derive(Debug)]
struct Carried {
    locales: Vec<DataLocale>,
    fallbacker: LocaleFallbackerBorrowed<'static>,
}

impl Carried {
    /// The locales a lookup in one of `self.locales` walks through for
    /// `marker`, as ICU4X's baked data walks them: the locale, then each
    /// its fallback gives for the marker's configuration, to the root.
    fn reached(&self, marker: DataMarkerInfo) -> HashSet<DataLocale> {
        let mut reached = HashSet::new();
        for locale in &self.locales {
            let mut walk = self
                .fallbacker
                .for_config(marker.fallback_config)
                .fallback_for(*locale);
            loop {
                reached.insert(*walk.get());
                if walk.get().is_unknown() {
                    break;
                }
                walk.step();
            }
        }
        reached
    }
}

impl DynamicDataProvider<ExportMarker> for Carried {
    fn load_data(
        &self,
        marker: DataMarkerInfo,
        req: DataRequest,
    ) -> Result<DataResponse<ExportMarker>, DataError> {
        Compiled.load_data(marker, req)
    }
}

impl IterableDynamicDataProvider<ExportMarker> for Carried {
    fn iter_ids_for_marker(
        &self,
        marker: DataMarkerInfo,
    ) -> Result<BTreeSet<DataIdentifierCow<'_>>, DataError> {
        let ids = Compiled.iter_ids_for_marker(marker)?;
        if marker.is_singleton {
            return Ok(ids);
        }
        let reached = self.reached(marker);
        Ok(ids
            .into_iter()
            .filter(|id| reached.contains(&id.locale))
            .collect())
    }
}

impl ExportableProvider for Carried {
    fn supported_markers(&self) -> BTreeSet<DataMarkerInfo> {
        Compiled.supported_markers()
    }
}

/// Why no data was exported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failed(pub String);

impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Failed {}

/// The carrier of the locales tagged `tags` (BCP 47, `en-GB`).
fn carried(tags: &[&str]) -> Result<Carried, Failed> {
    // The root is kept whatever the locales: it is where every lookup
    // ends, and all an application that declares none formats in.
    let mut locales = vec![DataLocale::default()];
    for tag in tags {
        let id = tag
            .parse::<LanguageIdentifier>()
            .map_err(|_| Failed(format!("`{tag}` isn't a BCP 47 language tag")))?;
        locales.push(DataLocale::from(&id));
    }
    Ok(Carried {
        locales,
        fallbacker: LocaleFallbacker::new(),
    })
}

/// The baked data a deployable carries for the locales tagged `tags`
/// (BCP 47, `en-GB`; none for an application that declares none), as
/// ICU4X's `ICU4X_DATA_DIR` reads it: `mod.rs` and a `.rs.data` file per
/// marker, by name. It is written in `scratch`, a folder replaced and
/// then removed. The same tags give the same bytes.
pub fn export(tags: &[&str], scratch: &Path) -> Result<BTreeMap<String, Vec<u8>>, Failed> {
    let carried = carried(tags)?;
    let failed = |error: DataError| Failed(format!("ICU4X couldn't export its data: {error}"));
    let mut options = Options::default();
    options.overwrite = true;
    let exporter = BakedExporter::new(scratch.to_path_buf(), options).map_err(failed)?;
    ExportDriver::new(
        [DataLocaleFamily::FULL],
        DeduplicationStrategy::None.into(),
        LocaleFallbacker::new().static_to_owned(),
    )
    // Every collation the compiled data has, search tables among them.
    .with_additional_collations(["search*".to_string()])
    .export(&carried, exporter)
    .map_err(failed)?;
    let mut files = BTreeMap::new();
    let read =
        |error: std::io::Error| Failed(format!("the exported data couldn't be read: {error}"));
    for entry in std::fs::read_dir(scratch).map_err(read)? {
        let path = entry.map_err(read)?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        files.insert(name.to_string(), std::fs::read(&path).map_err(read)?);
    }
    let _ = std::fs::remove_dir_all(scratch);
    Ok(files)
}

#[cfg(test)]
mod tests {
    // What an export keeps of the compiled data: a supported locale's
    // entries and those its lookups fall back to, collation among them,
    // every marker with one entry,
    // the root, and nothing else; the same bytes for the same locales.

    use super::*;

    use icu_collator::provider::CollationTailoringV1;
    use icu_datetime::provider::names::DatetimeNamesMonthGregorianV1;
    use icu_locale::provider::LocaleLikelySubtagsExtendedV1;

    /// A scratch folder of this test run's own.
    fn scratch(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("jadpo-cldr-{}-{name}", std::process::id()))
    }

    /// The locales `marker`'s kept entries are for, written as BCP 47 tags.
    fn kept(tags: &[&str], marker: DataMarkerInfo) -> BTreeSet<String> {
        let carried = carried(tags).expect("the tags read");
        carried
            .iter_ids_for_marker(marker)
            .expect("its entries")
            .into_iter()
            .map(|id| id.locale.to_string())
            .collect()
    }

    /// The locales the full compiled data has entries for, for `marker`.
    fn compiled(marker: DataMarkerInfo) -> BTreeSet<String> {
        Compiled
            .iter_ids_for_marker(marker)
            .expect("its entries")
            .into_iter()
            .map(|id| id.locale.to_string())
            .collect()
    }

    #[test]
    fn a_supported_locale_keeps_its_entries_and_those_its_lookups_fall_back_to() {
        let month = DatetimeNamesMonthGregorianV1::INFO;
        let kept = kept(&["en-GB", "fr-FR"], month);
        // British English falls back through the international English CLDR
        // calls `en-001` to `en`; French in France to `fr`; both to the root.
        for tag in ["en-001", "en", "fr", "und"] {
            assert!(kept.contains(tag), "{tag} in {kept:?}");
        }
        for tag in ["de", "fr-CA", "en-AU", "ja", "ar"] {
            assert!(compiled(month).contains(tag), "the full data has {tag}");
            assert!(!kept.contains(tag), "{tag} not in {kept:?}");
        }
        assert!(kept.len() < 10, "{kept:?}");
    }

    #[test]
    fn with_no_locales_only_the_root_is_kept() {
        let kept = kept(&[], DatetimeNamesMonthGregorianV1::INFO);
        assert_eq!(kept, BTreeSet::from(["und".to_string()]));
    }

    #[test]
    fn a_lookup_in_a_carried_locale_finds_the_entry_the_full_data_gives() {
        // Each marker keyed by locale, looked up in each locale as ICU4X's
        // baked data looks it up: the locale, then its fallback, until an
        // entry is there. The carried entries give the same one.
        let tags = [
            "en-GB",
            "fr-FR",
            "ar-EG",
            "zh-Hant-TW",
            "sr-Latn-RS",
            "pt-PT",
        ];
        let carried = carried(&tags).expect("the tags read");
        let fallbacker = LocaleFallbacker::new();
        for marker in Compiled.supported_markers() {
            if marker.is_singleton {
                continue;
            }
            let entries = |ids: BTreeSet<DataIdentifierCow<'_>>| {
                ids.into_iter()
                    .map(|id| (id.locale, id.marker_attributes.as_str().to_string()))
                    .collect::<HashSet<(DataLocale, String)>>()
            };
            let full = entries(Compiled.iter_ids_for_marker(marker).expect("entries"));
            let kept = entries(carried.iter_ids_for_marker(marker).expect("entries"));
            let attributes: BTreeSet<&String> =
                full.iter().map(|(_, attributes)| attributes).collect();
            for tag in tags {
                let locale = DataLocale::from(&tag.parse::<LanguageIdentifier>().expect("a tag"));
                for attributes in &attributes {
                    let found = |entries: &HashSet<(DataLocale, String)>| {
                        let mut walk = fallbacker
                            .for_config(marker.fallback_config)
                            .fallback_for(locale);
                        loop {
                            let at = *walk.get();
                            if entries.contains(&(at, (*attributes).clone())) {
                                return Some(at);
                            }
                            if at.is_unknown() {
                                return None;
                            }
                            walk.step();
                        }
                    };
                    assert_eq!(found(&kept), found(&full), "{marker:?} {tag} {attributes}");
                }
            }
        }
    }

    #[test]
    fn collation_is_kept_for_the_supported_locales() {
        // `list.sort_by` in a locale the app doesn't support orders by one
        // it does (area 10, "Locales"; chunk 8.4i), so only theirs is kept.
        let tailoring = CollationTailoringV1::INFO;
        assert!(compiled(tailoring).contains("sv"));
        let kept_for_two = kept(&["en-GB", "sv-SE"], tailoring);
        assert!(kept_for_two.contains("sv"), "{kept_for_two:?}");
        assert!(!kept_for_two.contains("de"), "{kept_for_two:?}");
        assert!(!kept(&["en-GB"], tailoring).contains("sv"));
        assert!(kept_for_two.len() < compiled(tailoring).len() / 4);
    }

    #[test]
    fn with_no_locales_only_the_roots_collation_is_kept() {
        // Sorting by a locale needs `locales { }`, so
        // an app with none keeps no locale's collation but the root's.
        let tailoring = CollationTailoringV1::INFO;
        let kept_for_none = kept(&[], tailoring);
        assert!(
            kept_for_none.iter().all(|locale| locale == "und"),
            "{kept_for_none:?}"
        );
    }

    #[test]
    fn a_marker_with_one_entry_is_kept() {
        let likely = LocaleLikelySubtagsExtendedV1::INFO;
        assert!(likely.is_singleton);
        assert_eq!(kept(&["en-GB"], likely), compiled(likely));
    }

    #[test]
    fn an_export_holds_every_marker_with_only_the_supported_locales_words() {
        let scratch = scratch("words");
        let english = export(&["en-GB"], &scratch).expect("exported");
        assert!(!scratch.exists(), "the scratch folder is removed");
        // `mod.rs` and one file per marker, each named in it.
        let module = String::from_utf8(english["mod.rs"].clone()).expect("text");
        assert_eq!(english.len(), Compiled.supported_markers().len() + 1);
        for name in english.keys().filter(|name| *name != "mod.rs") {
            assert!(module.contains(&format!("\"{name}\"")), "{name} included");
        }
        let months = "datetime_names_month_gregorian_v1.rs.data";
        let written = |files: &BTreeMap<String, Vec<u8>>| {
            String::from_utf8_lossy(&files[months]).into_owned()
        };
        assert!(written(&english).contains("September"));
        assert!(!written(&english).contains("septembre"));
        let french = export(&["en-GB", "fr-FR"], &scratch).expect("exported");
        assert!(written(&french).contains("septembre"));
        assert!(!written(&french).contains("septiembre"));
        assert!(
            english.values().map(Vec::len).sum::<usize>()
                < french.values().map(Vec::len).sum::<usize>()
        );
    }

    #[test]
    fn the_same_locales_give_the_same_bytes() {
        let scratch = scratch("same");
        let first = export(&["fr-FR", "en-GB"], &scratch).expect("exported");
        let again = export(&["fr-FR", "en-GB"], &scratch).expect("exported");
        assert!(first == again, "an export is reproducible");
    }

    #[test]
    fn a_tag_that_is_not_one_is_refused() {
        let scratch = scratch("refused");
        let refused = export(&["en_GB!"], &scratch).expect_err("refused");
        assert_eq!(refused.to_string(), "`en_GB!` isn't a BCP 47 language tag");
    }
}
