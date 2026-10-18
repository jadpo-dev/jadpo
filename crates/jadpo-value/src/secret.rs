//! Secret values (area 19, "Secret values", chunk 5.5r2): a `Password`,
//! an `ApiKey`, a `BearerToken`, a `TotpSecret`, a `SecretText` or a `Jwt`
//! as the runtime holds it. Its text is never written out: it shows as
//! `<redacted Password>` wherever a value is written for a person (a
//! report, `jadpo test`'s output, a debugger's view, `{:?}`), `==`
//! compares it in time that depends only on the lengths, never on where
//! two texts differ, and only the operations made for it read its text
//! ([`Secret::expose`]): `crypto.hash_password`, `verify_password`,
//! `verify_jwt` and `hash.sha256`. The checker keeps it out of responses,
//! failures, events, emails, storage and `json.write`; this is the
//! runtime's half, so a value that reached a writer anyway shows redacted.

use std::cmp::Ordering;
use std::fmt;
use std::rc::Rc;

use subtle::{Choice, ConstantTimeEq, ConstantTimeGreater};

/// A secret value: the standard type it is and its text.
#[derive(Clone)]
pub struct Secret {
    kind: &'static str,
    text: Rc<str>,
}

impl Secret {
    /// `text` held as a secret of standard type `kind` (`Password`).
    pub fn new(kind: &'static str, text: &str) -> Secret {
        Secret {
            kind,
            text: Rc::from(text),
        }
    }

    /// The standard type it is: `Password`.
    pub fn kind(&self) -> &'static str {
        self.kind
    }

    /// Its text, for the operations made to take it (hashing a password,
    /// checking a token) and nothing else: never for showing.
    pub fn expose(&self) -> &str {
        &self.text
    }
}

impl PartialEq for Secret {
    /// In constant time: the kinds, then the texts' bytes, every byte of
    /// the longer looked at whatever the first difference.
    fn eq(&self, other: &Secret) -> bool {
        self.kind == other.kind && constant_time_eq(self.text.as_bytes(), other.text.as_bytes())
    }
}

impl Eq for Secret {}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Secret({}, redacted)", self.kind)
    }
}

impl fmt::Display for Secret {
    /// `<redacted Password>`, as a value whose contents aren't shown is
    /// written (`<32 bytes>`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<redacted {}>", self.kind)
    }
}

/// Whether `a` and `b` are the same bytes, in time that depends on their
/// lengths alone: `subtle` compares two of one length byte by byte,
/// whatever the first difference, behind an optimisation barrier, and two
/// of different lengths differ, which says only their lengths.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    bool::from(a.ct_eq(b))
}

/// `a` against `b` byte by byte, a prefix first, in time that depends on
/// their lengths alone, for the canonical order a set or a map keeps
/// (area 2, "Set order"): the first difference decides, but every byte is
/// still looked at, through `subtle`'s comparisons.
pub fn constant_time_cmp(a: &[u8], b: &[u8]) -> Ordering {
    let mut less = Choice::from(0);
    let mut greater = Choice::from(0);
    for at in 0..a.len().max(b.len()) {
        // A missing byte is below every byte, so a prefix comes first:
        // 0 against a byte's 1 to 256.
        let x = a.get(at).map_or(0u16, |byte| u16::from(*byte) + 1);
        let y = b.get(at).map_or(0u16, |byte| u16::from(*byte) + 1);
        let undecided = !(less | greater);
        greater |= undecided & x.ct_gt(&y);
        less |= undecided & y.ct_gt(&x);
    }
    if bool::from(less) {
        Ordering::Less
    } else if bool::from(greater) {
        Ordering::Greater
    } else {
        Ordering::Equal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equality_is_by_kind_and_text() {
        let password = Secret::new("Password", "correct horse battery staple");
        assert_eq!(
            password,
            Secret::new("Password", "correct horse battery staple")
        );
        assert_ne!(
            password,
            Secret::new("Password", "correct horse battery stapler")
        );
        assert_ne!(
            password,
            Secret::new("SecretText", "correct horse battery staple")
        );
    }

    #[test]
    fn constant_time_equality_agrees_with_equality() {
        let texts: [&[u8]; 7] = [b"", b"a", b"b", b"ab", b"abc", b"abd", b"\0"];
        for a in texts {
            for b in texts {
                assert_eq!(constant_time_eq(a, b), a == b, "{a:?} and {b:?}");
                assert_eq!(constant_time_cmp(a, b), a.cmp(b), "{a:?} against {b:?}");
            }
        }
        // A difference in the first byte and in the last are both found,
        // and a shorter text that is a prefix differs.
        assert!(!constant_time_eq(b"Xbcdefgh", b"abcdefgh"));
        assert!(!constant_time_eq(b"abcdefgX", b"abcdefgh"));
        assert!(!constant_time_eq(b"abcdefg", b"abcdefgh"));
        assert_eq!(constant_time_cmp(b"\xff", b"\xff\0"), Ordering::Less);
    }

    #[test]
    fn it_is_never_written_out() {
        let password = Secret::new("Password", "correct horse battery staple");
        assert_eq!(password.to_string(), "<redacted Password>");
        assert_eq!(format!("{password:?}"), "Secret(Password, redacted)");
        assert_eq!(password.expose(), "correct horse battery staple");
    }
}
