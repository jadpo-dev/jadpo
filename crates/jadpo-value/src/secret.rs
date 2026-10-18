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
/// lengths alone: every byte up to the longer's length is compared,
/// whatever the first difference, and the differences are folded together
/// before one test at the end.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut difference = (a.len() ^ b.len()) as u64;
    for at in 0..a.len().max(b.len()) {
        let x = a.get(at).copied().unwrap_or(0);
        let y = b.get(at).copied().unwrap_or(0);
        difference |= u64::from(x ^ y);
    }
    std::hint::black_box(difference) == 0
}

/// `a` against `b` byte by byte, a prefix first, in time that depends on
/// their lengths alone, for the canonical order a set or a map keeps
/// (area 2, "Set order"): the first difference decides, but every byte is
/// still looked at.
pub fn constant_time_cmp(a: &[u8], b: &[u8]) -> Ordering {
    // -1, 0 or 1 once decided; `decided` is all ones from then on.
    let mut found: i32 = 0;
    let mut decided: i32 = 0;
    for at in 0..a.len().max(b.len()) {
        // A missing byte is below every byte, so a prefix comes first.
        let x = a.get(at).map_or(-1, |byte| i32::from(*byte));
        let y = b.get(at).map_or(-1, |byte| i32::from(*byte));
        let difference = x - y;
        let sign = i32::from(difference > 0) - i32::from(difference < 0);
        found |= sign & !decided;
        decided |= -i32::from(difference != 0);
    }
    std::hint::black_box(found).cmp(&0)
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
