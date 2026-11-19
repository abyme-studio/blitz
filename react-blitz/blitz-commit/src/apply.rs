use std::collections::HashMap;
use std::fmt;

use blitz_dom::{BaseDocument, DocumentMutator, LocalName, NodeId, QualName, ns};

use crate::decode::{DecodeError, decode};
use crate::ops::{Namespace, Op};

/// Why a commit was not applied whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    Decode(DecodeError),
    /// An op names a JS id that no node has: never created, or dropped.
    NoNode { id: u32, op: usize },
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApplyError::Decode(e) => e.fmt(f),
            ApplyError::NoNode { id, op } => {
                write!(f, "op {op} of the commit names node {id}, which does not exist")
            }
        }
    }
}

impl std::error::Error for ApplyError {}

impl From<DecodeError> for ApplyError {
    fn from(e: DecodeError) -> Self {
        ApplyError::Decode(e)
    }
}

/// The nodes JS knows by id, and which events each listens for.
#[derive(Default)]
pub struct Tree {
    /// Blitz's node for each JS id. Host-assigned IDs occupy the upper half of the u32 range.
    nodes: HashMap<u32, NodeId>,
    /// The JS id of each node JS knows.
    ids: HashMap<NodeId, u32>,
    /// The events each node's props handle.
    listeners: HashMap<NodeId, Vec<String>>,
}

impl Tree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Give JS the id `js` for a node it did not create, such as the container React renders into.
    pub fn bind(&mut self, js: u32, node: NodeId) {
        if let Some(old) = self.nodes.insert(js, node) {
            self.ids.remove(&old);
        }
        self.ids.insert(node, js);
    }

    /// The Blitz node JS calls `js`.
    pub fn node(&self, js: u32) -> Option<NodeId> {
        self.nodes.get(&js).copied()
    }

    /// The JS id of a Blitz node, when JS knows it.
    pub fn js_id(&self, node: NodeId) -> Option<u32> {
        self.ids.get(&node).copied()
    }

    /// The events the node's props handle, in the order JS first listened.
    pub fn listeners(&self, node: NodeId) -> &[String] {
        self.listeners.get(&node).map_or(&[], |events| events.as_slice())
    }

    /// Whether the node's props handle `event`.
    pub fn listens(&self, node: NodeId, event: &str) -> bool {
        self.listeners
            .get(&node)
            .is_some_and(|events| events.iter().any(|e| e == event))
    }

    /// Decode a commit and apply it through one `DocumentMutator`. A commit that does not decode
    /// changes nothing.
    pub fn apply(
        &mut self,
        doc: &mut BaseDocument,
        ops: &[u8],
        strings: &[u8],
    ) -> Result<(), ApplyError> {
        let ops = decode(ops, strings)?;
        let mut m = DocumentMutator::new(doc);
        for (index, op) in ops.iter().enumerate() {
            self.apply_op(&mut m, op, index)?;
        }
        Ok(())
    }

    fn get(&self, id: u32, op: usize) -> Result<NodeId, ApplyError> {
        self.node(id).ok_or(ApplyError::NoNode { id, op })
    }

    fn apply_op(
        &mut self,
        m: &mut DocumentMutator<'_>,
        op: &Op<'_>,
        index: usize,
    ) -> Result<(), ApplyError> {
        match *op {
            Op::CreateElement { id, tag, ns } => {
                let name = QualName::new(
                    None,
                    match ns {
                        Namespace::Html => ns!(html),
                        Namespace::Svg => ns!(svg),
                        Namespace::MathMl => ns!(mathml),
                    },
                    LocalName::from(tag),
                );
                let node = m.create_element(name, Vec::new());
                self.bind(id, node);
            }
            Op::CreateText { id, text } => {
                let node = m.create_text_node(text);
                self.bind(id, node);
            }
            Op::SetText { id, text } => {
                let node = self.get(id, index)?;
                m.set_node_text(node, text);
            }
            Op::SetAttr { id, name, value } => {
                let node = self.get(id, index)?;
                m.set_attribute(node, attr_name(name), value);
            }
            Op::RemoveAttr { id, name } => {
                let node = self.get(id, index)?;
                m.clear_attribute(node, attr_name(name));
            }
            Op::SetStyle { id, property, value } => {
                let node = self.get(id, index)?;
                m.set_style_property(node, property, value);
            }
            Op::RemoveStyle { id, property } => {
                let node = self.get(id, index)?;
                m.remove_style_property(node, property);
            }
            Op::Append { parent, child } => {
                let (parent, child) = (self.get(parent, index)?, self.get(child, index)?);
                m.append_children(parent, &[child]);
            }
            Op::InsertBefore { parent, child, before } => {
                let (parent, child, before) = (
                    self.get(parent, index)?,
                    self.get(child, index)?,
                    self.get(before, index)?,
                );
                if m.parent_id(before) == Some(parent) {
                    m.insert_nodes_before(before, &[child]);
                } else {
                    m.append_children(parent, &[child]);
                }
            }
            Op::Remove { parent: _, child } => {
                let child = self.get(child, index)?;
                m.remove_node(child);
            }
            Op::Drop { id } => {
                // A subtree's nodes are dropped with its root: a later `Drop` of one is nothing.
                let Some(node) = self.node(id) else {
                    return Ok(());
                };
                let (nodes, ids, listeners) = (&mut self.nodes, &mut self.ids, &mut self.listeners);
                m.remove_and_drop_node_with(node, &mut |dropped| {
                    if let Some(js) = ids.remove(&dropped) {
                        nodes.remove(&js);
                    }
                    listeners.remove(&dropped);
                });
            }
            Op::Listen { id, event, on } => {
                // A dropped node has no listeners: taking one off it is nothing, and the rest
                // of the commit still applies.
                if !on && self.node(id).is_none() {
                    return Ok(());
                }
                let node = self.get(id, index)?;
                let events = self.listeners.entry(node).or_default();
                let has = events.iter().position(|e| e == event);
                match (on, has) {
                    (true, None) => events.push(event.to_string()),
                    (false, Some(at)) => {
                        events.remove(at);
                    }
                    _ => {}
                }
            }
            Op::Popover { id, open } => {
                // A dropped node is out of the top layer already.
                if !open && self.node(id).is_none() {
                    return Ok(());
                }
                let node = self.get(id, index)?;
                match open {
                    true => m.doc.show_popover(node),
                    false => m.doc.hide_popover(node),
                }
            }
        }
        Ok(())
    }
}

fn attr_name(name: &str) -> QualName {
    QualName::new(None, ns!(), LocalName::from(name))
}
