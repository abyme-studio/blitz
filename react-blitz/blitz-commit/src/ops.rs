/// An op's code: the first word of each op in a commit.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpCode {
    /// `id tag namespace`
    CreateElement = 1,
    /// `id text`
    CreateText = 2,
    /// `id text`
    SetText = 3,
    /// `id name value`
    SetAttr = 4,
    /// `id name`
    RemoveAttr = 5,
    /// `id property value`, the property in CSS's kebab-case
    SetStyle = 6,
    /// `id property`
    RemoveStyle = 7,
    /// `parent child`
    Append = 8,
    /// `parent child before`
    InsertBefore = 9,
    /// `parent child`: detaches the child; it lives until its `Drop`
    Remove = 10,
    /// `id`: the node and its subtree are freed, and their ids forgotten
    Drop = 11,
    /// `id event on`: whether the node's props have a handler for `event` (`on` is 1 or 0)
    Listen = 12,
}

impl OpCode {
    pub fn from_u32(code: u32) -> Option<Self> {
        use OpCode::*;
        Some(match code {
            1 => CreateElement,
            2 => CreateText,
            3 => SetText,
            4 => SetAttr,
            5 => RemoveAttr,
            6 => SetStyle,
            7 => RemoveStyle,
            8 => Append,
            9 => InsertBefore,
            10 => Remove,
            11 => Drop,
            12 => Listen,
            _ => return None,
        })
    }

    /// How many argument words follow the code.
    pub fn arity(self) -> usize {
        use OpCode::*;
        match self {
            Drop => 1,
            RemoveAttr | RemoveStyle | Append | Remove | CreateText | SetText => 2,
            CreateElement | SetAttr | SetStyle | InsertBefore | Listen => 3,
        }
    }
}

/// An element's namespace, the third word of `CreateElement`.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Namespace {
    Html = 0,
    Svg = 1,
    MathMl = 2,
}

/// One decoded op, its strings borrowed from the commit's table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op<'a> {
    CreateElement { id: u32, tag: &'a str, ns: Namespace },
    CreateText { id: u32, text: &'a str },
    SetText { id: u32, text: &'a str },
    SetAttr { id: u32, name: &'a str, value: &'a str },
    RemoveAttr { id: u32, name: &'a str },
    SetStyle { id: u32, property: &'a str, value: &'a str },
    RemoveStyle { id: u32, property: &'a str },
    Append { parent: u32, child: u32 },
    InsertBefore { parent: u32, child: u32, before: u32 },
    Remove { parent: u32, child: u32 },
    Drop { id: u32 },
    Listen { id: u32, event: &'a str, on: bool },
}
