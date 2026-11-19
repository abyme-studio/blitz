use std::fmt;

use crate::ops::{Namespace, Op, OpCode};

/// Why a commit could not be read. Any of these is a bug in the encoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// `ops` is not a whole number of `u32` words.
    OpsNotWords(usize),
    /// The string table ends inside an entry.
    StringsTruncated { at: usize },
    /// A string in the table is not UTF-8.
    StringNotUtf8 { index: usize },
    /// An op names a string the table does not have.
    NoString { index: u32, at: usize },
    /// A word where an op code belongs is not one.
    UnknownOp { code: u32, at: usize },
    /// The ops end inside an op.
    OpTruncated { code: u32, at: usize },
    /// `CreateElement` names a namespace that does not exist.
    UnknownNamespace { ns: u32, at: usize },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use DecodeError::*;
        match self {
            OpsNotWords(len) => write!(f, "commit ops are {len} bytes, not whole u32 words"),
            StringsTruncated { at } => write!(f, "commit string table ends inside the entry at byte {at}"),
            StringNotUtf8 { index } => write!(f, "commit string {index} is not UTF-8"),
            NoString { index, at } => write!(f, "op at word {at} names string {index}, which the table does not have"),
            UnknownOp { code, at } => write!(f, "word {at} is op code {code}, which does not exist"),
            OpTruncated { code, at } => write!(f, "op {code} at word {at} ends before its arguments"),
            UnknownNamespace { ns, at } => write!(f, "op at word {at} names namespace {ns}, which does not exist"),
        }
    }
}

impl std::error::Error for DecodeError {}

fn read_strings(strings: &[u8]) -> Result<Vec<&str>, DecodeError> {
    let mut table = Vec::new();
    let mut at = 0;
    while at < strings.len() {
        let len_bytes = strings
            .get(at..at + 4)
            .ok_or(DecodeError::StringsTruncated { at })?;
        let len = u32::from_le_bytes(len_bytes.try_into().unwrap()) as usize;
        let bytes = strings
            .get(at + 4..at + 4 + len)
            .ok_or(DecodeError::StringsTruncated { at })?;
        let s = std::str::from_utf8(bytes).map_err(|_| DecodeError::StringNotUtf8 {
            index: table.len(),
        })?;
        table.push(s);
        at += 4 + len;
    }
    Ok(table)
}

/// Decode one commit into its ops, in order.
pub fn decode<'a>(ops: &[u8], strings: &'a [u8]) -> Result<Vec<Op<'a>>, DecodeError> {
    if ops.len() % 4 != 0 {
        return Err(DecodeError::OpsNotWords(ops.len()));
    }
    let words: Vec<u32> = ops
        .chunks_exact(4)
        .map(|w| u32::from_le_bytes(w.try_into().unwrap()))
        .collect();
    let table = read_strings(strings)?;

    let mut out = Vec::new();
    let mut at = 0;
    while at < words.len() {
        let code = words[at];
        let op = OpCode::from_u32(code).ok_or(DecodeError::UnknownOp { code, at })?;
        let args = words
            .get(at + 1..at + 1 + op.arity())
            .ok_or(DecodeError::OpTruncated { code, at })?;
        let s = |i: usize| {
            table
                .get(args[i] as usize)
                .copied()
                .ok_or(DecodeError::NoString { index: args[i], at })
        };
        out.push(match op {
            OpCode::CreateElement => Op::CreateElement {
                id: args[0],
                tag: s(1)?,
                ns: match args[2] {
                    0 => Namespace::Html,
                    1 => Namespace::Svg,
                    2 => Namespace::MathMl,
                    ns => return Err(DecodeError::UnknownNamespace { ns, at }),
                },
            },
            OpCode::CreateText => Op::CreateText { id: args[0], text: s(1)? },
            OpCode::SetText => Op::SetText { id: args[0], text: s(1)? },
            OpCode::SetAttr => Op::SetAttr { id: args[0], name: s(1)?, value: s(2)? },
            OpCode::RemoveAttr => Op::RemoveAttr { id: args[0], name: s(1)? },
            OpCode::SetStyle => Op::SetStyle { id: args[0], property: s(1)?, value: s(2)? },
            OpCode::RemoveStyle => Op::RemoveStyle { id: args[0], property: s(1)? },
            OpCode::Append => Op::Append { parent: args[0], child: args[1] },
            OpCode::InsertBefore => Op::InsertBefore {
                parent: args[0],
                child: args[1],
                before: args[2],
            },
            OpCode::Remove => Op::Remove { parent: args[0], child: args[1] },
            OpCode::Drop => Op::Drop { id: args[0] },
            OpCode::Listen => Op::Listen { id: args[0], event: s(1)?, on: args[2] != 0 },
            OpCode::Popover => Op::Popover { id: args[0], open: args[1] != 0 },
        });
        at += 1 + op.arity();
    }
    Ok(out)
}
