// Copyright 2026 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

use accesskit_atspi_common::{FullNodeId, PlatformNode};
use zbus::{fdo, interface, names::OwnedUniqueName};

use crate::atspi::{ObjectId, OwnedObjectAddress};

pub(crate) struct TableCellInterface {
    bus_name: OwnedUniqueName,
    node: PlatformNode,
}

impl TableCellInterface {
    pub fn new(bus_name: OwnedUniqueName, node: PlatformNode) -> Self {
        Self { bus_name, node }
    }

    fn map_error(&self) -> impl '_ + FnOnce(accesskit_atspi_common::Error) -> fdo::Error {
        |error| crate::util::map_error_from_node(&self.node, error)
    }

    fn addresses(&self, nodes: Vec<FullNodeId>) -> Vec<OwnedObjectAddress> {
        nodes
            .into_iter()
            .map(|node| {
                ObjectId::Node {
                    adapter: self.node.adapter_id(),
                    node,
                }
                .to_address(self.bus_name.inner())
            })
            .collect()
    }
}

#[interface(name = "org.a11y.atspi.TableCell")]
impl TableCellInterface {
    #[zbus(property)]
    fn position(&self) -> fdo::Result<(i32, i32)> {
        self.node.table_cell_position().map_err(self.map_error())
    }

    #[zbus(property)]
    fn row_span(&self) -> fdo::Result<i32> {
        self.node.table_cell_row_span().map_err(self.map_error())
    }

    #[zbus(property)]
    fn column_span(&self) -> fdo::Result<i32> {
        self.node.table_cell_column_span().map_err(self.map_error())
    }

    #[zbus(property)]
    fn table(&self) -> fdo::Result<OwnedObjectAddress> {
        let table = self.node.table_cell_table().map_err(self.map_error())?;
        Ok(super::optional_object_address(
            &self.bus_name,
            table.map(|node| ObjectId::Node {
                adapter: self.node.adapter_id(),
                node,
            }),
        )
        .0)
    }

    /// `(row, column, row_span, column_span)` as four out arguments, matching
    /// at-spi2-core's `TableCell.xml` and libatspi's `=>iiii` call.
    fn get_row_column_span(&self) -> fdo::Result<(i32, i32, i32, i32)> {
        self.node
            .table_cell_row_column_span()
            .map_err(self.map_error())
    }

    fn get_column_header_cells(&self) -> fdo::Result<Vec<OwnedObjectAddress>> {
        let cells = self
            .node
            .table_cell_column_header_cells()
            .map_err(self.map_error())?;
        Ok(self.addresses(cells))
    }

    fn get_row_header_cells(&self) -> fdo::Result<Vec<OwnedObjectAddress>> {
        let cells = self
            .node
            .table_cell_row_header_cells()
            .map_err(self.map_error())?;
        Ok(self.addresses(cells))
    }
}
