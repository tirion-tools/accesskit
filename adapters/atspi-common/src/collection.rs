// Copyright 2026 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

// org.a11y.atspi.Collection.GetMatches, following at-spi2-core's ATK adaptor
// (atk-adaptor/adaptors/collection-adaptor.c): pre-order walk over the filtered descendants of the
// queried object (the object itself is never a match), every criterion must hold, an invalid match
// type matches nothing, an empty criterion set matches everything for ALL/ANY/NONE. Orca's structural
// navigation (headings, tables, links, ...) and its "find" commands are built on this call.

use std::collections::HashMap;

use accesskit_consumer::{FullNodeId, NodeRef};

use crate::{filters::filter, node::NodeWrapper};

const MATCH_ALL: i32 = 1;
const MATCH_ANY: i32 = 2;
const MATCH_NONE: i32 = 3;
const MATCH_EMPTY: i32 = 4;

/// A decoded `(aiia{ss}iaiiasib)` match rule. States and roles arrive as bit arrays (two and four
/// 32-bit words, as libatspi's `_atspi_match_rule_marshal` writes them).
#[derive(Clone, Debug, Default)]
pub struct MatchRule {
    states: u64,
    states_mt: i32,
    attributes: Vec<(String, Vec<String>)>,
    attributes_mt: i32,
    roles: Vec<u32>,
    roles_mt: i32,
    interfaces: Vec<String>,
    interfaces_mt: i32,
    invert: bool,
}

/// Splits an attribute value at unescaped ':' (libatspi/Orca join alternatives with ':' and escape a
/// literal colon as "\:").
fn alternatives(value: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&':') => {
                out.last_mut().unwrap().push(':');
                chars.next();
            }
            ':' => out.push(String::new()),
            c => out.last_mut().unwrap().push(c),
        }
    }
    out
}

impl MatchRule {
    #[allow(clippy::too_many_arguments)]
    pub fn from_wire(
        states: &[i32],
        states_mt: i32,
        attributes: HashMap<String, String>,
        attributes_mt: i32,
        roles: &[i32],
        roles_mt: i32,
        interfaces: Vec<String>,
        interfaces_mt: i32,
        invert: bool,
    ) -> Self {
        let mut state_bits = 0u64;
        for (i, word) in states.iter().take(2).enumerate() {
            state_bits |= u64::from(*word as u32) << (32 * i);
        }
        let mut role_list = Vec::new();
        for (i, word) in roles.iter().take(4).enumerate() {
            for bit in 0..32 {
                if (*word as u32) & (1 << bit) != 0 {
                    role_list.push(i as u32 * 32 + bit);
                }
            }
        }
        let mut attributes: Vec<(String, Vec<String>)> = attributes
            .into_iter()
            .map(|(k, v)| (k, alternatives(&v)))
            .collect();
        attributes.sort();
        let interfaces = interfaces
            .into_iter()
            .map(|name| {
                name.strip_prefix("org.a11y.atspi.")
                    .unwrap_or(&name)
                    .to_ascii_lowercase()
            })
            .collect();
        Self {
            states: state_bits,
            states_mt,
            attributes,
            attributes_mt,
            roles: role_list,
            roles_mt,
            interfaces,
            interfaces_mt,
            invert,
        }
    }

    fn states_match(&self, states: u64) -> bool {
        let want = self.states;
        match self.states_mt {
            MATCH_ALL => states & want == want,
            MATCH_ANY => want == 0 || states & want != 0,
            MATCH_NONE => want == 0 || states & want == 0,
            MATCH_EMPTY => (want == 0 && states == 0) || (want != 0 && states & want == want),
            _ => false,
        }
    }

    fn roles_match(&self, role: u32) -> bool {
        match self.roles_mt {
            MATCH_ALL => match self.roles.as_slice() {
                [] => true,
                [only] => *only == role,
                _ => false,
            },
            MATCH_ANY => self.roles.is_empty() || self.roles.contains(&role),
            MATCH_NONE => !self.roles.contains(&role),
            MATCH_EMPTY => {
                self.roles.is_empty() || (self.roles.len() == 1 && self.roles[0] == role)
            }
            _ => false,
        }
    }

    fn attributes_match(&self, attributes: &HashMap<&'static str, String>) -> bool {
        let has = |(key, values): &(String, Vec<String>)| {
            attributes
                .get(key.as_str())
                .is_some_and(|value| values.iter().any(|v| v == value))
        };
        match self.attributes_mt {
            MATCH_ALL => self.attributes.iter().all(has),
            MATCH_ANY => self.attributes.is_empty() || self.attributes.iter().any(has),
            MATCH_NONE => !self.attributes.iter().any(has),
            MATCH_EMPTY => {
                if self.attributes.is_empty() {
                    attributes.is_empty()
                } else {
                    self.attributes.iter().all(has)
                }
            }
            _ => false,
        }
    }

    fn interfaces_match(&self, wrapper: &NodeWrapper) -> bool {
        let names: Vec<String> = wrapper
            .interfaces()
            .iter()
            .map(|iface| format!("{iface:?}").to_ascii_lowercase())
            .collect();
        let has = |name: &String| names.contains(name);
        match self.interfaces_mt {
            MATCH_ALL => self.interfaces.iter().all(has),
            MATCH_ANY => self.interfaces.is_empty() || self.interfaces.iter().any(has),
            MATCH_NONE => !self.interfaces.iter().any(has),
            MATCH_EMPTY => self.interfaces.iter().all(has),
            _ => false,
        }
    }

    fn matches(&self, node: &NodeRef, is_window_focused: bool) -> bool {
        let wrapper = NodeWrapper(node);
        let matched = self.roles_match(wrapper.role() as u32)
            && self.states_match(wrapper.state(is_window_focused).bits())
            && self.interfaces_match(&wrapper)
            && self.attributes_match(&wrapper.attributes());
        matched != self.invert
    }
}

/// Canonical (pre-order) matches among the filtered descendants of `node`; `count` 0 = unlimited.
/// `traverse` false restricts the search to the direct children, as in at-spi2-core.
pub(crate) fn collect_matches(
    node: &NodeRef,
    rule: &MatchRule,
    is_window_focused: bool,
    count: usize,
    traverse: bool,
    out: &mut Vec<FullNodeId>,
) {
    for child in node.filtered_children(&filter) {
        if count != 0 && out.len() >= count {
            return;
        }
        if rule.matches(&child, is_window_focused) {
            out.push(child.id());
        }
        if traverse {
            collect_matches(&child, rule, is_window_focused, count, traverse, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use accesskit::{Node, NodeId, Role};
    use atspi_common::Interface;

    use super::{MATCH_ALL, MATCH_ANY, MatchRule};
    use crate::{
        Adapter, PlatformNode,
        test_util::{adapter, node_at, with_children},
    };

    fn role_bits(roles: &[crate::Role]) -> Vec<i32> {
        let mut words = vec![0i32; 4];
        for role in roles {
            let bit = *role as u32;
            words[(bit / 32) as usize] |= (1u32 << (bit % 32)) as i32;
        }
        words
    }

    fn rule(roles: &[crate::Role], attributes: &[(&str, &str)], invert: bool) -> MatchRule {
        MatchRule::from_wire(
            &[0, 0],
            MATCH_ALL,
            attributes
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
            MATCH_ALL,
            &role_bits(roles),
            MATCH_ANY,
            Vec::new(),
            MATCH_ALL,
            invert,
        )
    }

    fn named(role: Role, label: &str, html_id: &str) -> Node {
        let mut node = Node::new(role);
        node.set_label(label);
        node.set_html_id(html_id);
        node
    }

    // Window > Document > [heading "First", group > [heading "Nested"], heading "Second"]
    fn document() -> Adapter {
        let mut group = named(Role::Group, "Group", "g");
        group.set_children(vec![NodeId(4)]);
        adapter(vec![
            (NodeId(0), with_children(Role::Window, &[NodeId(1)])),
            (
                NodeId(1),
                with_children(Role::Document, &[NodeId(2), NodeId(3), NodeId(5)]),
            ),
            (NodeId(2), named(Role::Heading, "First", "a")),
            (NodeId(3), group),
            (NodeId(4), named(Role::Heading, "Nested", "x:y")),
            (NodeId(5), named(Role::Heading, "Second", "b")),
        ])
    }

    fn names(
        a: &Adapter,
        node: &PlatformNode,
        rule: &MatchRule,
        reverse: bool,
        count: usize,
        traverse: bool,
    ) -> Vec<String> {
        node.collection_matches(rule, reverse, count, traverse)
            .unwrap()
            .into_iter()
            .map(|id| a.platform_node(id).name().unwrap())
            .collect()
    }

    #[test]
    fn collection_on_root_and_documents_only() {
        let a = document();
        assert!(
            node_at(&a, &[])
                .interfaces()
                .unwrap()
                .contains(Interface::Collection)
        );
        assert!(
            node_at(&a, &[0])
                .interfaces()
                .unwrap()
                .contains(Interface::Collection)
        );
        assert!(
            !node_at(&a, &[0, 0])
                .interfaces()
                .unwrap()
                .contains(Interface::Collection)
        );
    }

    #[test]
    fn role_match_in_canonical_and_reverse_order() {
        let a = document();
        let root = node_at(&a, &[]);
        let headings = rule(&[crate::Role::Heading], &[], false);
        assert_eq!(
            names(&a, &root, &headings, false, 0, true),
            ["First", "Nested", "Second"]
        );
        assert_eq!(
            names(&a, &root, &headings, true, 0, true),
            ["Second", "Nested", "First"]
        );
        assert_eq!(names(&a, &root, &headings, false, 1, true), ["First"]);
        // Without traverse only direct children are candidates.
        let doc = node_at(&a, &[0]);
        assert_eq!(
            names(&a, &doc, &headings, false, 0, false),
            ["First", "Second"]
        );
    }

    #[test]
    fn invert_and_attribute_alternatives() {
        let a = document();
        let doc = node_at(&a, &[0]);
        let not_headings = rule(&[crate::Role::Heading], &[], true);
        assert_eq!(names(&a, &doc, &not_headings, false, 0, true), ["Group"]);
        // ':' separates alternatives, "\:" is a literal colon.
        let by_id = rule(&[], &[("id", "b:x\\:y")], false);
        assert_eq!(
            names(&a, &doc, &by_id, false, 0, true),
            ["Nested", "Second"]
        );
    }
}
