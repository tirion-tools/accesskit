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
