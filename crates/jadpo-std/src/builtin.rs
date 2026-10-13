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
}

impl Builtin {
    pub const ALL: [Builtin; 14] = [
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
            Builtin::Email | Builtin::Url | Builtin::IpAddress => Builtin::Text,
            other => other,
        }
    }

    /// Whether it is raw material rather than a meaning: a named type
    /// built on it never widens to it, and a signature may not use it
    /// directly (area 2, question 11).
    pub fn is_representation(self) -> bool {
        !matches!(
            self,
            Builtin::Email | Builtin::Url | Builtin::IpAddress | Builtin::Unit
        )
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
    }
}
