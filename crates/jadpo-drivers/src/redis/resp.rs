//! RESP, the Redis serialization protocol, as the driver speaks it
//! (version 2, which every Redis-compatible service answers without being
//! asked for another): each command an array of bulk strings, and each
//! reply read as it arrives, a whole one at a time, from whatever bytes
//! have come so far.
//!
//! Replies are bounded: a bulk string or an array may claim no more than
//! [`MOST_REPLY_BYTES`] and arrays nest at most [`MOST_DEPTH`] deep, so a
//! server can't make the driver hold more than that, whatever it sends.

/// The most bytes one reply may hold: a cached entry is the largest thing
/// the driver reads, and none is near this.
pub const MOST_REPLY_BYTES: usize = 64 * 1024 * 1024;

/// How deep arrays may nest in one reply.
pub const MOST_DEPTH: usize = 16;

/// One reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// `+OK`.
    Status(String),
    /// `-ERR …`: the server refused the command; its message, which names
    /// what was wrong and never a value the command carried.
    Error(String),
    /// `:5`.
    Integer(i64),
    /// `$3\r\nabc`, or `$-1`, none.
    Bulk(Option<Vec<u8>>),
    /// `*2 …`, or `*-1`, none: a transaction that didn't run.
    Array(Option<Vec<Reply>>),
}

impl Reply {
    /// Whether it, or anything in it, is a refusal.
    pub fn refused(&self) -> Option<&str> {
        match self {
            Reply::Error(message) => Some(message),
            Reply::Array(Some(items)) => items.iter().find_map(Reply::refused),
            _ => None,
        }
    }
}

/// Bytes that aren't RESP, or a reply past the bounds: the connection
/// can't be read any further.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unreadable(pub &'static str);

/// `command` as RESP: an array of its words as bulk strings.
pub fn command<A: AsRef<[u8]>>(words: &[A]) -> Vec<u8> {
    let mut out = format!("*{}\r\n", words.len()).into_bytes();
    for word in words {
        let word = word.as_ref();
        out.extend_from_slice(format!("${}\r\n", word.len()).as_bytes());
        out.extend_from_slice(word);
        out.extend_from_slice(b"\r\n");
    }
    out
}

/// The first whole reply in `bytes` and how many bytes it took; `None`
/// while it hasn't all come.
pub fn parse(bytes: &[u8]) -> Result<Option<(Reply, usize)>, Unreadable> {
    let mut at = 0;
    match value(bytes, &mut at, 0)? {
        Some(reply) => Ok(Some((reply, at))),
        None => Ok(None),
    }
}

/// The value starting at `at`, moving `at` past it.
fn value(bytes: &[u8], at: &mut usize, depth: usize) -> Result<Option<Reply>, Unreadable> {
    if depth > MOST_DEPTH {
        return Err(Unreadable("its arrays nest too deep"));
    }
    let Some(line) = line(bytes, *at)? else {
        return Ok(None);
    };
    let (kind, rest) = line.split_first().ok_or(Unreadable("an empty line"))?;
    let after = *at + line.len() + 2;
    let text = || {
        std::str::from_utf8(rest)
            .map(str::to_string)
            .map_err(|_| Unreadable("a line that isn't UTF-8"))
    };
    let reply = match kind {
        b'+' => Reply::Status(text()?),
        b'-' => Reply::Error(text()?),
        b':' => Reply::Integer(number(rest)?),
        b'$' => {
            let length = number(rest)?;
            if length < 0 {
                *at = after;
                return Ok(Some(Reply::Bulk(None)));
            }
            let length = usize::try_from(length).map_err(|_| Unreadable("a length"))?;
            if length > MOST_REPLY_BYTES {
                return Err(Unreadable("a reply longer than the driver reads"));
            }
            let end = after + length;
            if bytes.len() < end + 2 {
                return Ok(None);
            }
            if bytes.get(end..end + 2) != Some(b"\r\n") {
                return Err(Unreadable("a bulk string that doesn't end its line"));
            }
            let body = bytes.get(after..end).ok_or(Unreadable("a length"))?;
            *at = end + 2;
            return Ok(Some(Reply::Bulk(Some(body.to_vec()))));
        }
        b'*' => {
            let length = number(rest)?;
            if length < 0 {
                *at = after;
                return Ok(Some(Reply::Array(None)));
            }
            let length = usize::try_from(length).map_err(|_| Unreadable("a length"))?;
            if length > MOST_REPLY_BYTES {
                return Err(Unreadable("an array longer than the driver reads"));
            }
            let mut next = after;
            // Never more room than the bytes could fill.
            let mut items = Vec::with_capacity(length.min(bytes.len() / 3 + 1));
            for _ in 0..length {
                match value(bytes, &mut next, depth + 1)? {
                    Some(item) => items.push(item),
                    None => return Ok(None),
                }
            }
            *at = next;
            return Ok(Some(Reply::Array(Some(items))));
        }
        _ => return Err(Unreadable("a reply of no kind RESP has")),
    };
    *at = after;
    Ok(Some(reply))
}

/// The line starting at `at`, without its CRLF; `None` until it has ended.
fn line(bytes: &[u8], at: usize) -> Result<Option<&[u8]>, Unreadable> {
    let Some(rest) = bytes.get(at..) else {
        return Ok(None);
    };
    match rest.windows(2).position(|pair| pair == b"\r\n") {
        Some(end) => Ok(rest.get(..end)),
        None if rest.len() > 512 => Err(Unreadable("a line longer than a header's")),
        None => Ok(None),
    }
}

fn number(digits: &[u8]) -> Result<i64, Unreadable> {
    std::str::from_utf8(digits)
        .ok()
        .and_then(|digits| digits.parse().ok())
        .ok_or(Unreadable("a number that isn't one"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_an_array_of_bulk_strings() {
        assert_eq!(
            command(&["SET", "jadpo:k", "0", "PX", "60000", "NX"]),
            b"*6\r\n$3\r\nSET\r\n$7\r\njadpo:k\r\n$1\r\n0\r\n$2\r\nPX\r\n$5\r\n60000\r\n$2\r\nNX\r\n"
        );
        assert_eq!(command::<&str>(&[]), b"*0\r\n");
    }

    #[test]
    fn each_kind_of_reply_is_read() {
        let replies: [(&[u8], Reply); 8] = [
            (b"+OK\r\n", Reply::Status("OK".into())),
            (
                b"-WRONGPASS invalid username-password pair\r\n",
                Reply::Error("WRONGPASS invalid username-password pair".into()),
            ),
            (b":-2\r\n", Reply::Integer(-2)),
            (b"$3\r\na\r\n\r\n", Reply::Bulk(Some(b"a\r\n".to_vec()))),
            (b"$-1\r\n", Reply::Bulk(None)),
            (b"*-1\r\n", Reply::Array(None)),
            (b"*0\r\n", Reply::Array(Some(Vec::new()))),
            (
                b"*3\r\n+OK\r\n:4\r\n:59000\r\n",
                Reply::Array(Some(vec![
                    Reply::Status("OK".into()),
                    Reply::Integer(4),
                    Reply::Integer(59_000),
                ])),
            ),
        ];
        for (bytes, reply) in replies {
            assert_eq!(parse(bytes), Ok(Some((reply, bytes.len()))), "{bytes:?}");
        }
    }

    /// A reply is read only once it has all come, however the bytes were
    /// split, and what follows it is left for the next.
    #[test]
    fn a_reply_is_read_once_it_has_all_come() {
        let whole = b"*2\r\n$5\r\nhello\r\n:7\r\n+QUEUED\r\n";
        let first = 19;
        for cut in 0..first {
            assert_eq!(parse(&whole[..cut]), Ok(None), "cut at {cut}");
        }
        let (reply, used) = parse(whole).unwrap().unwrap();
        assert_eq!(used, first);
        assert_eq!(
            reply,
            Reply::Array(Some(vec![
                Reply::Bulk(Some(b"hello".to_vec())),
                Reply::Integer(7)
            ]))
        );
        assert_eq!(
            parse(&whole[used..]),
            Ok(Some((Reply::Status("QUEUED".into()), 9)))
        );
    }

    #[test]
    fn what_isnt_resp_or_is_too_big_is_unreadable() {
        let refused: [&[u8]; 6] = [
            b"?what\r\n",
            b":seven\r\n",
            b"$3\r\nabcd\r\n",
            b"$99999999999\r\n",
            b"*99999999999\r\n",
            b"\r\n",
        ];
        for bytes in refused {
            assert!(parse(bytes).is_err(), "{bytes:?}");
        }
        let deep = "*1\r\n".repeat(MOST_DEPTH + 2);
        assert!(parse(deep.as_bytes()).is_err());
        assert!(parse(&[b'+'; 600]).is_err());
    }

    #[test]
    fn a_refusal_is_found_inside_a_transactions_answers() {
        let answered = Reply::Array(Some(vec![
            Reply::Status("OK".into()),
            Reply::Error(
                "WRONGTYPE Operation against a key holding the wrong kind of value".into(),
            ),
        ]));
        assert_eq!(
            answered.refused(),
            Some("WRONGTYPE Operation against a key holding the wrong kind of value")
        );
        assert_eq!(Reply::Integer(1).refused(), None);
    }
}
