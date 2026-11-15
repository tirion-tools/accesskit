// Copyright 2026 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

//! Helpers for unit tests that query `PlatformNode` like an AT-SPI client.

use accesskit::{ActionHandler, ActionRequest, Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};
use accesskit_consumer::FullNodeId;

use crate::{
    Adapter, AdapterCallback, AppContext, Event, InterfaceSet, PlatformNode, WindowBounds,
};

struct NoOpCallback;

impl AdapterCallback for NoOpCallback {
    fn register_interfaces(&self, _: &Adapter, _: FullNodeId, _: InterfaceSet) {}
    fn unregister_interfaces(&self, _: &Adapter, _: FullNodeId, _: InterfaceSet) {}
    fn emit_event(&self, _: &Adapter, _: Event) {}
}

struct NoOpActionHandler;

impl ActionHandler for NoOpActionHandler {
    fn do_action(&mut self, _: ActionRequest) {}
}

/// An adapter over `nodes`; `NodeId(0)` is the root and has focus.
pub(crate) fn adapter(nodes: Vec<(NodeId, Node)>) -> Adapter {
    Adapter::new(
        &AppContext::new(None),
        NoOpCallback,
        TreeUpdate {
            nodes,
            tree: Some(TreeInfo::new(NodeId(0))),
            tree_id: TreeId::ROOT,
            focus: NodeId(0),
        },
        false,
        WindowBounds::default(),
        NoOpActionHandler,
    )
}

/// The platform node reached from the root by following `path`, a list of
/// child indices in the filtered (AT-SPI) tree.
pub(crate) fn node_at(adapter: &Adapter, path: &[usize]) -> PlatformNode {
    let mut node = adapter.platform_node(adapter.root_id());
    for &index in path {
        let id = node.child_at_index(index).unwrap().unwrap();
        node = adapter.platform_node(id);
    }
    node
}

pub(crate) fn with_children(role: Role, children: &[NodeId]) -> Node {
    let mut node = Node::new(role);
    node.set_children(children.to_vec());
    node
}

/// A text run holding `text`, one character per byte (tests use ASCII).
pub(crate) fn text_run(text: &str) -> Node {
    let mut node = Node::new(Role::TextRun);
    node.set_value(text);
    node.set_character_lengths(vec![1; text.len()]);
    node
}
