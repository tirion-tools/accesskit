// Copyright 2026 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

use std::collections::HashMap;

use accesskit_atspi_common::{MatchRule, PlatformNode};
use zbus::{fdo, interface, names::OwnedUniqueName};

use crate::atspi::{ObjectId, OwnedObjectAddress};

/// at-spi2-core `Collection.xml` `MatchRule`: `(aiia{ss}iaiiasib)`.
type WireRule = (
    Vec<i32>,
    i32,
    HashMap<String, String>,
    i32,
    Vec<i32>,
    i32,
    Vec<String>,
    i32,
    bool,
);

const SORT_ORDER_REVERSE_CANONICAL: u32 = 4;

pub(crate) struct CollectionInterface {
    bus_name: OwnedUniqueName,
    node: PlatformNode,
}

impl CollectionInterface {
    pub fn new(bus_name: OwnedUniqueName, node: PlatformNode) -> Self {
        Self { bus_name, node }
    }

    fn map_error(&self) -> impl '_ + FnOnce(accesskit_atspi_common::Error) -> fdo::Error {
        |error| crate::util::map_error_from_node(&self.node, error)
    }
}

#[interface(name = "org.a11y.atspi.Collection")]
impl CollectionInterface {
    /// Only GetMatches is implemented (the call Orca's structural navigation makes); sort orders
    /// other than canonical and reverse canonical are treated as canonical, as in at-spi2-core.
    fn get_matches(
        &self,
        rule: WireRule,
        sortby: u32,
        count: i32,
        traverse: bool,
    ) -> fdo::Result<Vec<OwnedObjectAddress>> {
        let (
            states,
            states_mt,
            attributes,
            attributes_mt,
            roles,
            roles_mt,
            interfaces,
            interfaces_mt,
            invert,
        ) = rule;
        let rule = MatchRule::from_wire(
            &states,
            states_mt,
            attributes,
            attributes_mt,
            &roles,
            roles_mt,
            interfaces,
            interfaces_mt,
            invert,
        );
        let matches = self
            .node
            .collection_matches(
                &rule,
                sortby == SORT_ORDER_REVERSE_CANONICAL,
                usize::try_from(count).unwrap_or(0),
                traverse,
            )
            .map_err(self.map_error())?;
        Ok(matches
            .into_iter()
            .map(|node| {
                ObjectId::Node {
                    adapter: self.node.adapter_id(),
                    node,
                }
                .to_address(self.bus_name.inner())
            })
            .collect())
    }
}
