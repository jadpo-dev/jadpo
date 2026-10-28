//! The closed lists of commerce, health, travel and the other published
//! standards (area 19, "The standard types"): each
//! a built-in enum, read ignoring case and written as its authority's
//! code, in the order of its codes. A list whose codes are written the same
//! in every language (`FOB`, `B2`, `O-`) is shown by its code; one whose
//! codes are words (`premium_economy`) by its authority's English wording,
//! in every locale, since CLDR names none of them, as a subdivision is
//! shown by its English name (chunk 5.5l).

use super::places::ClosedList;
use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, NO_OPTIONS, StandardType, Storage,
    TextForm,
};
use crate::read::SqlType;

/// `Incoterm` (ICC's Incoterms 2020).
pub const INCOTERM: &str = "Incoterm";
/// `BloodType` (ABO and RhD).
pub const BLOOD_TYPE: &str = "BloodType";
/// `Allergen` (the UK's and EU's 14).
pub const ALLERGEN: &str = "Allergen";
/// `CabinClass`.
pub const CABIN_CLASS: &str = "CabinClass";
/// `CefrLevel` (the Council of Europe's CEFR).
pub const CEFR_LEVEL: &str = "CefrLevel";
/// `EpcRating` (the UK's energy performance bands).
pub const EPC_RATING: &str = "EpcRating";
/// `Tenure` (land in England and Wales).
pub const TENURE: &str = "Tenure";
/// `LawfulBasis` (UK GDPR and EU GDPR, Article 6).
pub const LAWFUL_BASIS: &str = "LawfulBasis";

/// The categories of these lists that are personal data, by name (area
/// 15): a blood type is a person's health.
pub const PERSONAL: &[(&str, &str)] = &[(BLOOD_TYPE, "health")];

/// `Incoterm`'s variants: the eleven rules of Incoterms 2020 and the five
/// earlier editions used that 2020 dropped, retired, by code.
pub const INCOTERMS: &[(&str, &str)] = &[
    ("cfr", "CFR"),
    ("cif", "CIF"),
    ("cip", "CIP"),
    ("cpt", "CPT"),
    ("daf", "DAF"),
    ("dap", "DAP"),
    ("dat", "DAT"),
    ("ddp", "DDP"),
    ("ddu", "DDU"),
    ("deq", "DEQ"),
    ("des", "DES"),
    ("dpu", "DPU"),
    ("exw", "EXW"),
    ("fas", "FAS"),
    ("fca", "FCA"),
    ("fob", "FOB"),
];

/// The Incoterms rules earlier editions had and Incoterms 2020 doesn't:
/// `DAT`, which 2020 renamed `DPU`, and the four Incoterms 2010 replaced.
/// None needs an English name, since a rule is shown by its code.
pub const INCOTERMS_RETIRED: &[(&str, &str)] = &[
    ("daf", ""),
    ("dat", ""),
    ("ddu", ""),
    ("deq", ""),
    ("des", ""),
];

/// `BloodType`'s variants: the ABO group and RhD, written as a blood
/// service writes them, by code.
pub const BLOOD_TYPES: &[(&str, &str)] = &[
    ("a_positive", "A+"),
    ("a_negative", "A-"),
    ("ab_positive", "AB+"),
    ("ab_negative", "AB-"),
    ("b_positive", "B+"),
    ("b_negative", "B-"),
    ("o_positive", "O+"),
    ("o_negative", "O-"),
];

/// `Allergen`'s variants: the 14 the UK's Food Information Regulations
/// 2014 and the EU's Regulation 1169/2011 (Annex II) say a label must
/// name.
pub const ALLERGEN_CODES: &[(&str, &str)] = &[
    ("celery", "celery"),
    ("cereals_containing_gluten", "cereals_containing_gluten"),
    ("crustaceans", "crustaceans"),
    ("eggs", "eggs"),
    ("fish", "fish"),
    ("lupin", "lupin"),
    ("milk", "milk"),
    ("molluscs", "molluscs"),
    ("mustard", "mustard"),
    ("peanuts", "peanuts"),
    ("sesame", "sesame"),
    ("soybeans", "soybeans"),
    ("sulphites", "sulphites"),
    ("tree_nuts", "tree_nuts"),
];

/// Each allergen as the Food Standards Agency words it, in `ALLERGEN_CODES`'
/// order.
pub const ALLERGEN_NAMES: &[&str] = &[
    "Celery",
    "Cereals containing gluten",
    "Crustaceans",
    "Eggs",
    "Fish",
    "Lupin",
    "Milk",
    "Molluscs",
    "Mustard",
    "Peanuts",
    "Sesame",
    "Soybeans",
    "Sulphur dioxide and sulphites",
    "Tree nuts",
];

/// `CabinClass`'s variants: the four cabins IATA's fare classes are sold
/// in.
pub const CABIN_CLASSES: &[(&str, &str)] = &[
    ("business", "business"),
    ("economy", "economy"),
    ("first", "first"),
    ("premium_economy", "premium_economy"),
];

/// Each cabin as an airline names it, in `CABIN_CLASSES`' order.
pub const CABIN_CLASS_NAMES: &[&str] = &["Business", "Economy", "First", "Premium economy"];

/// `CefrLevel`'s variants: the Common European Framework of Reference for
/// Languages' six levels.
pub const CEFR_LEVELS: &[(&str, &str)] = &[
    ("a1", "A1"),
    ("a2", "A2"),
    ("b1", "B1"),
    ("b2", "B2"),
    ("c1", "C1"),
    ("c2", "C2"),
];

/// `EpcRating`'s variants: an energy performance certificate's bands, from
/// A, the most efficient, to G (the Energy Performance of Buildings
/// (England and Wales) Regulations 2012).
pub const EPC_RATINGS: &[(&str, &str)] = &[
    ("a", "A"),
    ("b", "B"),
    ("c", "C"),
    ("d", "D"),
    ("e", "E"),
    ("f", "F"),
    ("g", "G"),
];

/// `Tenure`'s variants: how land in England and Wales is held, as HM Land
/// Registry registers it.
pub const TENURES: &[(&str, &str)] = &[
    ("commonhold", "commonhold"),
    ("freehold", "freehold"),
    ("leasehold", "leasehold"),
];

/// Each tenure as HM Land Registry names it, in `TENURES`' order.
pub const TENURE_NAMES: &[&str] = &["Commonhold", "Freehold", "Leasehold"];

/// `LawfulBasis`'s variants: the six bases GDPR's Article 6(1) gives for
/// processing personal data.
pub const LAWFUL_BASES: &[(&str, &str)] = &[
    ("consent", "consent"),
    ("contract", "contract"),
    ("legal_obligation", "legal_obligation"),
    ("legitimate_interests", "legitimate_interests"),
    ("public_task", "public_task"),
    ("vital_interests", "vital_interests"),
];

/// Each basis as the ICO names it, in `LAWFUL_BASES`' order.
pub const LAWFUL_BASIS_NAMES: &[&str] = &[
    "Consent",
    "Contract",
    "Legal obligation",
    "Legitimate interests",
    "Public task",
    "Vital interests",
];

/// These lists, each with its authority and the codes it withdrew.
pub const CLOSED_LISTS: &[ClosedList] = &[
    list(INCOTERM, "ICC Incoterms 2020", INCOTERMS_RETIRED),
    list(BLOOD_TYPE, "ABO and RhD", &[]),
    list(ALLERGEN, "UK FIR 2014", &[]),
    list(CABIN_CLASS, "IATA", &[]),
    list(CEFR_LEVEL, "CEFR", &[]),
    list(EPC_RATING, "UK EPC", &[]),
    list(TENURE, "HM Land Registry", &[]),
    list(LAWFUL_BASIS, "GDPR", &[]),
];

const fn list(
    ty: &'static str,
    authority: &'static str,
    retired: &'static [(&'static str, &'static str)],
) -> ClosedList {
    ClosedList {
        ty,
        authority,
        forms: &[],
        retired,
    }
}

/// A closed list held as an enum, written as its code and shown by `key`.
const fn closed(
    name: &'static str,
    codes: &'static [(&'static str, &'static str)],
    key: FormatKey,
    valid: &'static [(&'static str, &'static str)],
    malformed: &'static [&'static str],
) -> StandardType {
    StandardType {
        name,
        holds: Holds::Enum(codes),
        text: Some(TextForm::Code),
        json: Json::Text,
        storage: Storage::Column(SqlType::Text),
        format: Format {
            key,
            options: NO_OPTIONS,
        },
        ordered: false,
        validator: None,
        shape: None,
        examples: Examples {
            valid,
            format: None,
            malformed,
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
        signals: crate::standard::NO_SIGNALS,
    }
}

/// A rule of Incoterms 2020 for who carries a shipment's costs and risks,
/// shown by its code, as traders write it in every language.
pub const INCOTERM_ENTRY: StandardType = closed(
    INCOTERM,
    INCOTERMS,
    FormatKey::Canonical,
    &[("DAT", "DAT"), ("EXW", "EXW"), ("FOB", "FOB")],
    &["F.O.B.", "FOB 2020", "FO", "XYZ", ""],
);

/// A person's blood group, ABO and RhD, shown by its code: `O-`.
pub const BLOOD_TYPE_ENTRY: StandardType = closed(
    BLOOD_TYPE,
    BLOOD_TYPES,
    FormatKey::Canonical,
    &[("AB+", "AB+"), ("O-", "O-")],
    &["O pos", "0+", "O", "AB", ""],
);

/// One of the 14 allergens a food label must name, shown by the Food
/// Standards Agency's wording.
pub const ALLERGEN_ENTRY: StandardType = closed(
    ALLERGEN,
    ALLERGEN_CODES,
    FormatKey::Names(ALLERGEN_NAMES),
    &[
        ("cereals_containing_gluten", "Cereals containing gluten"),
        ("milk", "Milk"),
        ("sulphites", "Sulphur dioxide and sulphites"),
    ],
    &["gluten", "nuts", "Cereals containing gluten", ""],
);

/// The cabin a seat is sold in, shown by the cabin's English name.
pub const CABIN_CLASS_ENTRY: StandardType = closed(
    CABIN_CLASS,
    CABIN_CLASSES,
    FormatKey::Names(CABIN_CLASS_NAMES),
    &[
        ("economy", "Economy"),
        ("premium_economy", "Premium economy"),
    ],
    &["coach", "premium economy", "J", ""],
);

/// A level of the CEFR, shown by its code: `B2`.
pub const CEFR_LEVEL_ENTRY: StandardType = closed(
    CEFR_LEVEL,
    CEFR_LEVELS,
    FormatKey::Canonical,
    &[("A1", "A1"), ("B2", "B2"), ("C2", "C2")],
    &["C3", "B", "native", ""],
);

/// An energy performance certificate's band, shown by its letter.
pub const EPC_RATING_ENTRY: StandardType = closed(
    EPC_RATING,
    EPC_RATINGS,
    FormatKey::Canonical,
    &[("A", "A"), ("G", "G")],
    &["H", "A+", "92", ""],
);

/// How land is held, shown by HM Land Registry's name for it.
pub const TENURE_ENTRY: StandardType = closed(
    TENURE,
    TENURES,
    FormatKey::Names(TENURE_NAMES),
    &[("freehold", "Freehold"), ("leasehold", "Leasehold")],
    &["share_of_freehold", "F", "Freehold estate", ""],
);

/// A lawful basis for processing personal data, shown by the ICO's name
/// for it.
pub const LAWFUL_BASIS_ENTRY: StandardType = closed(
    LAWFUL_BASIS,
    LAWFUL_BASES,
    FormatKey::Names(LAWFUL_BASIS_NAMES),
    &[
        ("consent", "Consent"),
        ("legitimate_interests", "Legitimate interests"),
    ],
    &["legitimate interests", "6(1)(f)", "f", ""],
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_list_is_sorted_by_code_and_named_in_full() {
        for codes in [
            INCOTERMS,
            BLOOD_TYPES,
            ALLERGEN_CODES,
            CABIN_CLASSES,
            CEFR_LEVELS,
            EPC_RATINGS,
            TENURES,
            LAWFUL_BASES,
        ] {
            let written: Vec<&str> = codes.iter().map(|(_, code)| *code).collect();
            let mut sorted = written.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(written, sorted, "sorted by code, each once");
        }
        for (codes, names) in [
            (ALLERGEN_CODES, ALLERGEN_NAMES),
            (CABIN_CLASSES, CABIN_CLASS_NAMES),
            (TENURES, TENURE_NAMES),
            (LAWFUL_BASES, LAWFUL_BASIS_NAMES),
        ] {
            assert_eq!(codes.len(), names.len());
        }
        // Incoterms 2020's eleven, the 14 allergens.
        assert_eq!(INCOTERMS.len() - INCOTERMS_RETIRED.len(), 11);
        assert_eq!(ALLERGEN_CODES.len(), 14);
    }
}
