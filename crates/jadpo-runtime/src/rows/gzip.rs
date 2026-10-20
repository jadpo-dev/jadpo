//! A gzip file unpacked as a file helper reads it (area 16, "Files"; area
//! 12, "Rows and formats"; chunk 6.4b3): only when the helper is given
//! `max_unpacked_size:`, which the caller holds the unpacked bytes to, and
//! pure, fed the packed bytes a window at a time and giving the unpacked
//! ones as far as it is asked.
//!
//! A gzip file is one member or more, one after another (RFC 1952,
//! section 2.2): each a header, its deflate data and a trailer holding the
//! CRC-32 and the size, modulo 2³², of what it unpacks to. Each header is
//! read whole, with every optional part it says it has, its own CRC-16
//! checked when it has one; each trailer is held to what its member
//! unpacked to. The deflate data is miniz_oxide's to unpack. Anything that
//! isn't so is damage, and a file that ends inside a member is cut short:
//! both stop the run. No other compression, and no archive, is opened.

use miniz_oxide::inflate::stream::{InflateState, inflate};
use miniz_oxide::{DataFormat, MZError, MZFlush, MZStatus};

/// How long a member's header may be before it is taken for damage: its
/// fixed ten bytes, an extra field of at most 65,535 and a name and a
/// comment of reasonable length.
const LONGEST_HEADER: usize = 1 << 20;

/// What stops an unpacking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GzipFault {
    /// What it holds isn't gzip as RFC 1952 writes it.
    Damaged,
    /// It ends inside a member.
    Cut,
}

/// Where in its members an unpacking is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    /// Gathering a member's header.
    Header,
    /// Unpacking its deflate data.
    Data,
    /// Gathering its trailer.
    Trailer,
    /// After a member: another one, or the file's end.
    Between,
}

/// One gzip file's unpacking, fed its packed bytes in order.
pub struct Unpacker {
    part: Part,
    /// The header or trailer gathered so far.
    gathered: Vec<u8>,
    inflate: Box<InflateState>,
    /// The CRC-32 and the size of what the member in hand unpacked to.
    crc: u32,
    size: u64,
}

impl std::fmt::Debug for Unpacker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Unpacker")
            .field("part", &self.part)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl Default for Unpacker {
    fn default() -> Self {
        Unpacker::new()
    }
}

/// How one call of [`Unpacker::unpack`] went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unpacked {
    /// How many of the packed bytes it was given it used: the rest are
    /// given again with what follows them.
    pub used: usize,
    /// Every member is unpacked and the file has ended.
    pub done: bool,
}

impl Unpacker {
    pub fn new() -> Unpacker {
        Unpacker {
            part: Part::Header,
            gathered: Vec::new(),
            inflate: InflateState::new_boxed(DataFormat::Raw),
            crc: 0,
            size: 0,
        }
    }

    /// Unpacks what it can of `packed`, the file's bytes from where the
    /// last call stopped using them, adding at most `room` bytes to `out`.
    /// `ends` says `packed` runs to the file's end. It stops once `out`
    /// has had `room` bytes, once `packed` is used up, or once the file is
    /// done.
    pub fn unpack(
        &mut self,
        packed: &[u8],
        ends: bool,
        out: &mut Vec<u8>,
        room: usize,
    ) -> Result<Unpacked, GzipFault> {
        let mut used = 0usize;
        let mut written = 0usize;
        loop {
            let rest = packed.get(used..).unwrap_or_default();
            match self.part {
                Part::Between => {
                    if rest.is_empty() {
                        return Ok(Unpacked { used, done: ends });
                    }
                    self.part = Part::Header;
                }
                Part::Header => {
                    let before = self.gathered.len();
                    self.gathered.extend_from_slice(rest);
                    match header_length(&self.gathered)? {
                        Some(length) => {
                            used += length.saturating_sub(before);
                            self.gathered.clear();
                            self.inflate = InflateState::new_boxed(DataFormat::Raw);
                            self.crc = 0;
                            self.size = 0;
                            self.part = Part::Data;
                        }
                        None => {
                            used += rest.len();
                            if self.gathered.len() > LONGEST_HEADER {
                                return Err(GzipFault::Damaged);
                            }
                            return if ends {
                                Err(GzipFault::Cut)
                            } else {
                                Ok(Unpacked { used, done: false })
                            };
                        }
                    }
                }
                Part::Data => {
                    let space = room.saturating_sub(written);
                    if space == 0 {
                        return Ok(Unpacked { used, done: false });
                    }
                    let mut chunk = vec![0u8; space.min(1 << 16)];
                    let result = inflate(&mut self.inflate, rest, &mut chunk, MZFlush::None);
                    let made = chunk.get(..result.bytes_written).unwrap_or_default();
                    self.crc = crc32(self.crc, made);
                    self.size += made.len() as u64;
                    out.extend_from_slice(made);
                    written += made.len();
                    used += result.bytes_consumed;
                    match result.status {
                        Ok(MZStatus::StreamEnd) => self.part = Part::Trailer,
                        Ok(MZStatus::Ok) => {
                            if result.bytes_consumed == 0 && result.bytes_written == 0 {
                                return self.starved(ends, used);
                            }
                        }
                        Err(MZError::Buf) => return self.starved(ends, used),
                        Ok(MZStatus::NeedDict) | Err(_) => return Err(GzipFault::Damaged),
                    }
                }
                Part::Trailer => {
                    let wanted = 8 - self.gathered.len();
                    let taken = rest.get(..wanted.min(rest.len())).unwrap_or_default();
                    self.gathered.extend_from_slice(taken);
                    used += taken.len();
                    if self.gathered.len() < 8 {
                        return if ends {
                            Err(GzipFault::Cut)
                        } else {
                            Ok(Unpacked { used, done: false })
                        };
                    }
                    let word = |at: usize| {
                        self.gathered
                            .get(at..at + 4)
                            .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
                            .map(u32::from_le_bytes)
                    };
                    let (Some(crc), Some(size)) = (word(0), word(4)) else {
                        return Err(GzipFault::Damaged);
                    };
                    // The size is kept modulo 2³² (RFC 1952, section 2.3.1).
                    if crc != self.crc || u64::from(size) != self.size & 0xffff_ffff {
                        return Err(GzipFault::Damaged);
                    }
                    self.gathered.clear();
                    self.part = Part::Between;
                }
            }
        }
    }

    /// The deflate data wants more of the file than it was given: more,
    /// or, at its end, the file was cut short.
    fn starved(&self, ends: bool, used: usize) -> Result<Unpacked, GzipFault> {
        if ends {
            Err(GzipFault::Cut)
        } else {
            Ok(Unpacked { used, done: false })
        }
    }
}

/// How long the member header `bytes` starts with is: `None` while it
/// isn't whole, and damage when it isn't a gzip header (RFC 1952, section
/// 2.3): its magic number, deflate, no reserved flag, and, when it has
/// one, its CRC-16 right.
fn header_length(bytes: &[u8]) -> Result<Option<usize>, GzipFault> {
    const FHCRC: u8 = 0x02;
    const FEXTRA: u8 = 0x04;
    const FNAME: u8 = 0x08;
    const FCOMMENT: u8 = 0x10;
    // What a header's first bytes may be, as far as they go.
    let opening = [0x1f, 0x8b, 0x08];
    for (byte, expected) in bytes.iter().zip(opening) {
        if *byte != expected {
            return Err(GzipFault::Damaged);
        }
    }
    let Some(&flags) = bytes.get(3) else {
        return Ok(None);
    };
    if flags & 0xe0 != 0 {
        return Err(GzipFault::Damaged);
    }
    let mut at = 10usize;
    if bytes.len() < at {
        return Ok(None);
    }
    if flags & FEXTRA != 0 {
        let Some(length) = bytes
            .get(at..at + 2)
            .and_then(|pair| <[u8; 2]>::try_from(pair).ok())
            .map(u16::from_le_bytes)
        else {
            return Ok(None);
        };
        at += 2 + usize::from(length);
        if bytes.len() < at {
            return Ok(None);
        }
    }
    for flag in [FNAME, FCOMMENT] {
        if flags & flag != 0 {
            match bytes
                .get(at..)
                .and_then(|rest| rest.iter().position(|b| *b == 0))
            {
                Some(zero) => at += zero + 1,
                None => return Ok(None),
            }
        }
    }
    if flags & FHCRC != 0 {
        let Some(stored) = bytes
            .get(at..at + 2)
            .and_then(|pair| <[u8; 2]>::try_from(pair).ok())
            .map(u16::from_le_bytes)
        else {
            return Ok(None);
        };
        let header = bytes.get(..at).unwrap_or_default();
        if u32::from(stored) != crc32(0, header) & 0xffff {
            return Err(GzipFault::Damaged);
        }
        at += 2;
    }
    Ok(Some(at))
}

/// CRC-32 as gzip keeps it (ISO 3309, the reflected polynomial
/// `0xedb88320`), carried on from `crc` over `bytes`, half a byte at a
/// time.
pub fn crc32(crc: u32, bytes: &[u8]) -> u32 {
    let mut crc = !crc;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..2 {
            let index = (crc & 0xf) as usize;
            crc = (crc >> 4) ^ NIBBLES.get(index).copied().unwrap_or(0);
        }
    }
    !crc
}

/// The CRC-32 remainder of each half byte.
const NIBBLES: [u32; 16] = [
    0x0000_0000,
    0x1db7_1064,
    0x3b6e_20c8,
    0x26d9_30ac,
    0x76dc_4190,
    0x6b6b_51f4,
    0x4db2_6158,
    0x5005_713c,
    0xedb8_8320,
    0xf00f_9344,
    0xd6d6_a3e8,
    0xcb61_b38c,
    0x9b64_c2b0,
    0x86d3_d2d4,
    0xa00a_e278,
    0xbdbd_f21c,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// `gzip.compress` of a seed list, 770 bytes, `mtime=0`.
    const PACKED: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0x65, 0xce, 0xb1, 0x0d, 0x83,
        0x40, 0x00, 0x04, 0xc1, 0xdc, 0x55, 0xb8, 0x00, 0x82, 0xbf, 0x3b, 0xb0, 0xa1, 0x0b, 0x5a,
        0xb0, 0xe0, 0x03, 0x64, 0x81, 0x2d, 0x21, 0xfa, 0x47, 0xa4, 0x6c, 0x3a, 0xd1, 0x4c, 0xbf,
        0xb9, 0x36, 0xdb, 0x67, 0xad, 0x8f, 0xb1, 0x94, 0xd2, 0x8c, 0xc7, 0xfa, 0xff, 0x2e, 0xdb,
        0x73, 0xaf, 0x75, 0xde, 0x2f, 0x12, 0xc9, 0xa4, 0x90, 0x5a, 0x52, 0x47, 0x7a, 0x91, 0xde,
        0xa4, 0x9e, 0x34, 0x80, 0xc4, 0xbd, 0xb8, 0x17, 0xf7, 0xe2, 0x5e, 0xdc, 0x8b, 0x7b, 0x71,
        0x2f, 0xee, 0xc5, 0xbd, 0xb8, 0x37, 0xf7, 0xe6, 0xde, 0xdc, 0x9b, 0x7b, 0x73, 0x6f, 0xee,
        0xcd, 0xbd, 0xb9, 0x37, 0xf7, 0xe6, 0x3e, 0xdc, 0x87, 0xfb, 0x70, 0x1f, 0xee, 0xc3, 0x7d,
        0xb8, 0x0f, 0xf7, 0xe1, 0x3e, 0xdc, 0xe7, 0xbe, 0x3f, 0x01, 0x86, 0x86, 0x99, 0x1c, 0x02,
        0x03, 0x00, 0x00,
    ];

    /// Two members, `first,` then `second\n`.
    const MEMBERS: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0x4b, 0xcb, 0x2c, 0x2a, 0x2e,
        0xd1, 0x01, 0x00, 0xd7, 0x36, 0x47, 0x15, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x8b, 0x08, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0x2b, 0x4e, 0x4d, 0xce, 0xcf, 0x4b, 0xe1, 0x02, 0x00,
        0x7e, 0xc0, 0x0f, 0x06, 0x07, 0x00, 0x00, 0x00,
    ];

    /// `flags\n` behind a header with an extra field, a name, a comment
    /// and its own CRC-16.
    const FLAGGED: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x1e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x04, 0x00, 0x61, 0x62, 0x01,
        0x00, 0x73, 0x65, 0x65, 0x64, 0x73, 0x2e, 0x63, 0x73, 0x76, 0x00, 0x61, 0x20, 0x63, 0x6f,
        0x6d, 0x6d, 0x65, 0x6e, 0x74, 0x00, 0x0b, 0x01, 0x4b, 0xcb, 0x49, 0x4c, 0x2f, 0xe6, 0x02,
        0x00, 0x40, 0x59, 0x68, 0x19, 0x06, 0x00, 0x00, 0x00,
    ];

    fn seeds() -> Vec<u8> {
        let mut text = b"code,name\n".to_vec();
        for n in 0..40 {
            text.extend_from_slice(format!("P{n:03},Pumpkin seeds\n").as_bytes());
        }
        text
    }

    /// Unpacks `packed` fed `window` bytes at a time, asking for `room`
    /// bytes at a time, as a run reads a file.
    fn unpacked(packed: &[u8], window: usize, room: usize) -> Result<Vec<u8>, GzipFault> {
        let mut unpacker = Unpacker::new();
        let mut out = Vec::new();
        let mut held: Vec<u8> = Vec::new();
        let mut read = 0usize;
        loop {
            let ends = read >= packed.len();
            let Unpacked { used, done } = unpacker.unpack(&held, ends, &mut out, room)?;
            held.drain(..used);
            if done {
                return Ok(out);
            }
            if used == 0 || held.is_empty() {
                let next = packed.get(read..(read + window).min(packed.len())).unwrap();
                held.extend_from_slice(next);
                read += next.len();
            }
        }
    }

    #[test]
    fn a_member_unpacks_however_its_bytes_and_room_are_cut() {
        assert_eq!(crc32(0, b"123456789"), 0xcbf4_3926);
        for window in [1, 3, 7, 64, 1000] {
            for room in [1, 5, 100, 1 << 16] {
                assert_eq!(
                    unpacked(PACKED, window, room).as_deref(),
                    Ok(seeds().as_slice()),
                    "{window} {room}"
                );
            }
        }
        assert_eq!(
            unpacked(MEMBERS, 4, 3).as_deref(),
            Ok(&b"first,second\n"[..])
        );
        assert_eq!(unpacked(FLAGGED, 2, 64).as_deref(), Ok(&b"flags\n"[..]));
    }

    #[test]
    fn damage_and_a_file_cut_short_stop_it() {
        // The trailer's CRC, its size, a reserved flag, the header's CRC-16
        // and what follows a member.
        for at in [PACKED.len() - 8, PACKED.len() - 1, 3, FLAGGED.len()] {
            let mut damaged = if at == FLAGGED.len() {
                FLAGGED.to_vec()
            } else {
                PACKED.to_vec()
            };
            match at {
                3 => damaged[3] = 0x20,
                n if n == FLAGGED.len() => damaged[36] ^= 1,
                n => damaged[n] ^= 1,
            }
            assert_eq!(unpacked(&damaged, 16, 64), Err(GzipFault::Damaged), "{at}");
        }
        let mut trailing = MEMBERS.to_vec();
        trailing.extend_from_slice(b"\0\0");
        assert_eq!(unpacked(&trailing, 16, 64), Err(GzipFault::Damaged));
        for cut in [5, 12, PACKED.len() - 9, PACKED.len() - 3] {
            assert_eq!(
                unpacked(&PACKED[..cut], 16, 64),
                Err(GzipFault::Cut),
                "{cut}"
            );
        }
    }
}
