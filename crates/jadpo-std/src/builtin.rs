//! The types built into the language (area 2): no Jadpo declaration can say
//! what they are made of, so the compiler and the runtime both know them
//! from here.

/// A built-in type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Builtin {
    Bool,
    Int,
    Decimal,
    Text,
    Bytes,
    Uuid,
    Email,
    Url,
    IpAddress,
    Instant,
    CalendarDate,
    Time,
    Duration,
    Unit,
    /// The opaque cursor a page of results gives for the next one (area
    /// 6, "Order and pages"): text to the program, read only by the query
    /// that made it.
    PageCursor,
    /// How many records a page holds: an `Int` from 1 to 100 (area 6).
    PageSize,
    /// An API token as `sessions.issue_token` gives it, once (area 8):
    /// text an `Output` can send, which no entity can keep.
    ApiToken,
    /// A service key as `Service.issue_key` gives it, once (area 8).
    ServiceKey,
}

/// `PageSize`'s own rules: from 1 to 100 (area 6, "Order and pages").
static PAGE_SIZE_RULES: crate::Rules = crate::Rules {
    min: Some(crate::Num {
        mantissa: 1,
        scale: 0,
    }),
    max: Some(crate::Num {
        mantissa: 100,
        scale: 0,
    }),
    min_length: None,
    max_length: None,
    pattern: None,
    format: None,
    scale: None,
    version: None,
    min_duration: None,
    max_duration: None,
};

impl Builtin {
    pub const ALL: [Builtin; 18] = [
        Builtin::Bool,
        Builtin::Int,
        Builtin::Decimal,
        Builtin::Text,
        Builtin::Bytes,
        Builtin::Uuid,
        Builtin::Email,
        Builtin::Url,
        Builtin::IpAddress,
        Builtin::Instant,
        Builtin::CalendarDate,
        Builtin::Time,
        Builtin::Duration,
        Builtin::Unit,
        Builtin::PageCursor,
        Builtin::PageSize,
        Builtin::ApiToken,
        Builtin::ServiceKey,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Builtin::Bool => "Bool",
            Builtin::Int => "Int",
            Builtin::Decimal => "Decimal",
            Builtin::Text => "Text",
            Builtin::Bytes => "Bytes",
            Builtin::Uuid => "Uuid",
            Builtin::Email => "Email",
            Builtin::Url => "Url",
            Builtin::IpAddress => "IpAddress",
            Builtin::Instant => "Instant",
            Builtin::CalendarDate => "CalendarDate",
            Builtin::Time => "Time",
            Builtin::Duration => "Duration",
            Builtin::Unit => "Unit",
            Builtin::PageCursor => "PageCursor",
            Builtin::PageSize => "PageSize",
            Builtin::ApiToken => "ApiToken",
            Builtin::ServiceKey => "ServiceKey",
        }
    }

    pub fn from_name(name: &str) -> Option<Builtin> {
        Builtin::ALL
            .into_iter()
            .find(|builtin| builtin.name() == name)
    }

    /// What its values are made of: the validated text types are text.
    pub fn representation(self) -> Builtin {
        match self {
            Builtin::Email
            | Builtin::Url
            | Builtin::IpAddress
            | Builtin::PageCursor
            | Builtin::ApiToken
            | Builtin::ServiceKey => Builtin::Text,
            Builtin::PageSize => Builtin::Int,
            other => other,
        }
    }

    /// Whether it is raw material rather than a meaning: a named type
    /// built on it never widens to it, and a signature may not use it
    /// directly (area 2, question 11).
    pub fn is_representation(self) -> bool {
        !matches!(
            self,
            Builtin::Email
                | Builtin::Url
                | Builtin::IpAddress
                | Builtin::Unit
                | Builtin::PageCursor
                | Builtin::PageSize
                | Builtin::ApiToken
                | Builtin::ServiceKey
        )
    }

    /// The rules the type itself carries, below any a declaration adds:
    /// `PageSize`'s range.
    pub fn rules(self) -> Option<&'static crate::Rules> {
        match self {
            Builtin::PageSize => Some(&PAGE_SIZE_RULES),
            _ => None,
        }
    }

    pub fn is_number(self) -> bool {
        matches!(self, Builtin::Int | Builtin::Decimal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_read_back() {
        for builtin in Builtin::ALL {
            assert_eq!(Builtin::from_name(builtin.name()), Some(builtin));
        }
        assert_eq!(Builtin::Email.representation(), Builtin::Text);
        assert!(!Builtin::Url.is_representation());
        assert_eq!(Builtin::PageSize.representation(), Builtin::Int);
        assert_eq!(
            Builtin::PageSize.rules().and_then(|rules| rules.max),
            Some(crate::Num::whole(100))
        );
    }
}
