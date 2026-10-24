//! The opcode reader: splits a pickle stream into opcodes and checks every length and frame.
//!
//! The reader allocates nothing: an [`Op`] is the opcode byte and byte ranges into the input.
//! Lengths are checked against the input before anything is read; a protocol 4+ `FRAME` must
//! hold whole opcodes, as Python's own unpickler requires.

use saveprism_core::limits::len_u64;
use saveprism_core::{Error, Result};

use super::PICKLE_PARSE;

pub(crate) const MARK: u8 = b'(';
pub(crate) const STOP: u8 = b'.';
pub(crate) const POP: u8 = b'0';
pub(crate) const POP_MARK: u8 = b'1';
pub(crate) const DUP: u8 = b'2';
pub(crate) const FLOAT: u8 = b'F';
pub(crate) const INT: u8 = b'I';
pub(crate) const BININT: u8 = b'J';
pub(crate) const BININT1: u8 = b'K';
pub(crate) const LONG: u8 = b'L';
pub(crate) const BININT2: u8 = b'M';
pub(crate) const NONE: u8 = b'N';
pub(crate) const PERSID: u8 = b'P';
pub(crate) const BINPERSID: u8 = b'Q';
pub(crate) const REDUCE: u8 = b'R';
pub(crate) const STRING: u8 = b'S';
pub(crate) const BINSTRING: u8 = b'T';
pub(crate) const SHORT_BINSTRING: u8 = b'U';
pub(crate) const UNICODE: u8 = b'V';
pub(crate) const BINUNICODE: u8 = b'X';
pub(crate) const APPEND: u8 = b'a';
pub(crate) const BUILD: u8 = b'b';
pub(crate) const GLOBAL: u8 = b'c';
pub(crate) const DICT: u8 = b'd';
pub(crate) const EMPTY_DICT: u8 = b'}';
pub(crate) const APPENDS: u8 = b'e';
pub(crate) const GET: u8 = b'g';
pub(crate) const BINGET: u8 = b'h';
pub(crate) const INST: u8 = b'i';
pub(crate) const LONG_BINGET: u8 = b'j';
pub(crate) const LIST: u8 = b'l';
pub(crate) const EMPTY_LIST: u8 = b']';
pub(crate) const OBJ: u8 = b'o';
pub(crate) const PUT: u8 = b'p';
pub(crate) const BINPUT: u8 = b'q';
pub(crate) const LONG_BINPUT: u8 = b'r';
pub(crate) const SETITEM: u8 = b's';
pub(crate) const TUPLE: u8 = b't';
pub(crate) const EMPTY_TUPLE: u8 = b')';
pub(crate) const SETITEMS: u8 = b'u';
pub(crate) const BINFLOAT: u8 = b'G';
pub(crate) const PROTO: u8 = 0x80;
pub(crate) const NEWOBJ: u8 = 0x81;
pub(crate) const EXT1: u8 = 0x82;
pub(crate) const EXT2: u8 = 0x83;
pub(crate) const EXT4: u8 = 0x84;
pub(crate) const TUPLE1: u8 = 0x85;
pub(crate) const TUPLE2: u8 = 0x86;
pub(crate) const TUPLE3: u8 = 0x87;
pub(crate) const NEWTRUE: u8 = 0x88;
pub(crate) const NEWFALSE: u8 = 0x89;
pub(crate) const LONG1: u8 = 0x8a;
pub(crate) const LONG4: u8 = 0x8b;
pub(crate) const BINBYTES: u8 = b'B';
pub(crate) const SHORT_BINBYTES: u8 = b'C';
pub(crate) const SHORT_BINUNICODE: u8 = 0x8c;
pub(crate) const BINUNICODE8: u8 = 0x8d;
pub(crate) const BINBYTES8: u8 = 0x8e;
pub(crate) const EMPTY_SET: u8 = 0x8f;
pub(crate) const ADDITEMS: u8 = 0x90;
pub(crate) const FROZENSET: u8 = 0x91;
pub(crate) const NEWOBJ_EX: u8 = 0x92;
pub(crate) const STACK_GLOBAL: u8 = 0x93;
pub(crate) const MEMOIZE: u8 = 0x94;
pub(crate) const FRAME: u8 = 0x95;
pub(crate) const BYTEARRAY8: u8 = 0x96;
pub(crate) const NEXT_BUFFER: u8 = 0x97;
pub(crate) const READONLY_BUFFER: u8 = 0x98;

/// How the argument of an opcode is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layout {
    /// No argument.
    Bare,
    /// A fixed number of bytes.
    Fixed(usize),
    /// One line ending in `\n`.
    Line,
    /// Two lines.
    TwoLines,
    /// A little-endian unsigned length of this many bytes, then that many bytes.
    Counted(usize),
    /// A little-endian signed 32-bit length (negative is an error), then that many bytes.
    SignedCounted,
}

fn layout(code: u8) -> Option<Layout> {
    Some(match code {
        MARK | STOP | POP | POP_MARK | DUP | NONE | BINPERSID | REDUCE | APPEND | BUILD | DICT
        | EMPTY_DICT | APPENDS | LIST | EMPTY_LIST | OBJ | SETITEM | TUPLE | EMPTY_TUPLE
        | SETITEMS | NEWOBJ | TUPLE1 | TUPLE2 | TUPLE3 | NEWTRUE | NEWFALSE | EMPTY_SET
        | ADDITEMS | FROZENSET | NEWOBJ_EX | STACK_GLOBAL | MEMOIZE | NEXT_BUFFER
        | READONLY_BUFFER => Layout::Bare,
        BININT1 | BINGET | BINPUT | PROTO | EXT1 => Layout::Fixed(1),
        BININT2 | EXT2 => Layout::Fixed(2),
        BININT | LONG_BINGET | LONG_BINPUT | EXT4 => Layout::Fixed(4),
        BINFLOAT | FRAME => Layout::Fixed(8),
        FLOAT | INT | LONG | PERSID | STRING | UNICODE | GET | PUT => Layout::Line,
        GLOBAL | INST => Layout::TwoLines,
        SHORT_BINSTRING | SHORT_BINBYTES | SHORT_BINUNICODE | LONG1 => Layout::Counted(1),
        BINUNICODE | BINBYTES => Layout::Counted(4),
        BINUNICODE8 | BINBYTES8 | BYTEARRAY8 => Layout::Counted(8),
        BINSTRING | LONG4 => Layout::SignedCounted,
        _ => return None,
    })
}

/// One opcode: where it is and where its argument is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Op {
    /// The opcode byte.
    pub code: u8,
    /// Offset of the opcode byte.
    pub start: usize,
    /// One past the last byte of the opcode.
    pub end: usize,
    /// Offset of the argument data (after a length prefix, if any).
    pub data: usize,
}

/// Reads opcodes one by one from the start of a pickle stream.
pub(crate) struct Reader<'a> {
    input: &'a [u8],
    pos: usize,
    /// End of the current protocol 4 frame, if one is open.
    frame_end: Option<usize>,
    stopped: bool,
}

pub(crate) fn parse_error(message: impl Into<String>, offset: usize) -> Error {
    Error::new(PICKLE_PARSE, message).with_offset(len_u64(offset))
}

fn read_le(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .rev()
        .fold(0_u64, |acc, byte| (acc << 8) | u64::from(*byte))
}

impl<'a> Reader<'a> {
    pub(crate) fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            pos: 0,
            frame_end: None,
            stopped: false,
        }
    }

    fn take(&self, from: usize, len: usize, what: &str) -> Result<&'a [u8]> {
        let end = from
            .checked_add(len)
            .ok_or_else(|| parse_error(format!("the {what} is too long"), from))?;
        self.input
            .get(from..end)
            .ok_or_else(|| parse_error(format!("the {what} is cut short"), from))
    }

    /// The end of the line that starts at `from` (one past its `\n`).
    fn line_end(&self, from: usize) -> Result<usize> {
        let rest = self.input.get(from..).unwrap_or_default();
        match rest.iter().position(|byte| *byte == b'\n') {
            Some(at) => Ok(from + at + 1),
            None => Err(parse_error("a text argument has no end of line", from)),
        }
    }

    /// The next opcode, or `None` after `STOP`. `STOP` must be the last byte of the input.
    pub(crate) fn next_op(&mut self) -> Result<Option<Op>> {
        if self.stopped {
            return Ok(None);
        }
        let start = self.pos;
        if self.frame_end == Some(start) {
            self.frame_end = None;
        }
        let Some(&code) = self.input.get(start) else {
            return Err(parse_error("the pickle ends without a STOP opcode", start));
        };
        let after = start + 1;
        let shape = layout(code)
            .ok_or_else(|| parse_error(format!("unknown opcode 0x{code:02x}"), start))?;
        let (data, end) = match shape {
            Layout::Bare => (after, after),
            Layout::Fixed(len) => {
                self.take(after, len, "opcode argument")?;
                (after, after + len)
            }
            Layout::Line => (after, self.line_end(after)?),
            Layout::TwoLines => {
                let first = self.line_end(after)?;
                (after, self.line_end(first)?)
            }
            Layout::Counted(width) => {
                let prefix = self.take(after, width, "length")?;
                let len = usize::try_from(read_le(prefix))
                    .map_err(|_| parse_error("a length does not fit in memory", after))?;
                let data = after + width;
                self.take(data, len, "data")?;
                (data, data + len)
            }
            Layout::SignedCounted => {
                let prefix = self.take(after, 4, "length")?;
                let raw = <[u8; 4]>::try_from(prefix)
                    .map_err(|_| parse_error("the length is cut short", after))?;
                let len = usize::try_from(i32::from_le_bytes(raw))
                    .map_err(|_| parse_error("a length is negative", after))?;
                let data = after + 4;
                self.take(data, len, "data")?;
                (data, data + len)
            }
        };
        if let Some(frame_end) = self.frame_end
            && end > frame_end
        {
            return Err(parse_error(
                "an opcode runs past the end of its frame",
                start,
            ));
        }
        if code == FRAME {
            if self.frame_end.is_some() {
                return Err(parse_error(
                    "a frame starts before the current frame ends",
                    start,
                ));
            }
            let len = usize::try_from(read_le(self.take(data, 8, "frame length")?))
                .map_err(|_| parse_error("the frame length does not fit in memory", start))?;
            self.take(end, len, "frame")?;
            self.frame_end = Some(end + len);
        }
        if code == STOP {
            if end != self.input.len() {
                return Err(parse_error("data follows the STOP opcode", end));
            }
            self.stopped = true;
        }
        self.pos = end;
        Ok(Some(Op {
            code,
            start,
            end,
            data,
        }))
    }

    /// The argument bytes of `op` (without a length prefix; a line keeps its `\n`).
    pub(crate) fn arg(&self, op: &Op) -> &'a [u8] {
        self.input.get(op.data..op.end).unwrap_or_default()
    }
}

/// Unsigned little-endian number of up to 8 bytes.
pub(crate) fn le_u64(bytes: &[u8]) -> u64 {
    read_le(bytes)
}

/// A line argument without its `\n`.
pub(crate) fn line(arg: &[u8]) -> &[u8] {
    arg.strip_suffix(b"\n").unwrap_or(arg)
}

/// The two lines of a `GLOBAL` / `INST` argument.
pub(crate) fn two_lines(arg: &[u8]) -> Option<(&[u8], &[u8])> {
    let first = arg.iter().position(|byte| *byte == b'\n')?;
    let module = arg.get(..first)?;
    let name = line(arg.get(first + 1..)?);
    Some((module, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(input: &[u8]) -> Result<Vec<u8>> {
        let mut reader = Reader::new(input);
        let mut out = Vec::new();
        while let Some(op) = reader.next_op()? {
            out.push(op.code);
        }
        Ok(out)
    }

    #[test]
    fn reads_every_layout() {
        let input =
            b"\x80\x02K\x05M\x01\x02J\x01\x02\x03\x04I12\nU\x02abX\x01\x00\x00\x00zcm\nn\nN.";
        assert_eq!(
            codes(input).unwrap(),
            vec![
                PROTO,
                BININT1,
                BININT2,
                BININT,
                INT,
                SHORT_BINSTRING,
                BINUNICODE,
                GLOBAL,
                NONE,
                STOP
            ]
        );
    }

    #[test]
    fn rejects_bad_streams() {
        for input in [
            &b""[..],
            b"K",
            b"X\xff\xff\xff\xff",
            b"T\xff\xff\xff\xffab.",
            b"I12",
            b"N",
            b"N.N",
            b"\x99.",
            b"\x95\x10\x00\x00\x00\x00\x00\x00\x00N.",
        ] {
            assert!(codes(input).is_err(), "{input:?}");
        }
    }

    #[test]
    fn frames_must_hold_whole_opcodes() {
        // A frame of 3 bytes holding K 1 and N; STOP outside the frame.
        assert!(codes(b"\x95\x03\x00\x00\x00\x00\x00\x00\x00K\x01N.").is_ok());
        // The frame ends inside the BININT1 argument.
        assert!(codes(b"\x95\x01\x00\x00\x00\x00\x00\x00\x00K\x01.").is_err());
        // A second frame starts before the first one ends.
        assert!(
            codes(b"\x95\x0a\x00\x00\x00\x00\x00\x00\x00\x95\x00\x00\x00\x00\x00\x00\x00\x00.")
                .is_err()
        );
    }
}
