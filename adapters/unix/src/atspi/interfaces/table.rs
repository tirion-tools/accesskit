// Copyright 2026 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

use accesskit_atspi_common::{FullNodeId, PlatformNode};
use zbus::{fdo, interface, names::OwnedUniqueName};

use crate::atspi::{ObjectId, OwnedObjectAddress};

pub(crate) struct TableInterface {
    bus_name: OwnedUniqueName,
    node: PlatformNode,
}

impl TableInterface {
    pub fn new(bus_name: OwnedUniqueName, node: PlatformNode) -> Self {
        Self { bus_name, node }
    }

    fn map_error(&self) -> impl '_ + FnOnce(accesskit_atspi_common::Error) -> fdo::Error {
        |error| crate::util::map_error_from_node(&self.node, error)
    }

    fn address(&self, node: Option<FullNodeId>) -> (OwnedObjectAddress,) {
        super::optional_object_address(
            &self.bus_name,
            node.map(|node| ObjectId::Node {
                adapter: self.node.adapter_id(),
                node,
            }),
        )
    }
}

#[interface(name = "org.a11y.atspi.Table")]
impl TableInterface {
    #[zbus(property)]
    fn n_rows(&self) -> fdo::Result<i32> {
        self.node.table_row_count().map_err(self.map_error())
    }

    #[zbus(property)]
    fn n_columns(&self) -> fdo::Result<i32> {
        self.node.table_column_count().map_err(self.map_error())
    }

    #[zbus(property)]
    fn caption(&self) -> fdo::Result<OwnedObjectAddress> {
        let caption = self.node.table_caption().map_err(self.map_error())?;
        Ok(self.address(caption).0)
    }

    #[zbus(property)]
    fn summary(&self) -> OwnedObjectAddress {
        OwnedObjectAddress::null()
    }

    #[zbus(property)]
    fn n_selected_rows(&self) -> i32 {
        0
    }

    #[zbus(property)]
    fn n_selected_columns(&self) -> i32 {
        0
    }

    fn get_accessible_at(&self, row: i32, column: i32) -> fdo::Result<(OwnedObjectAddress,)> {
        let cell = self
            .node
            .table_cell_at(row, column)
            .map_err(self.map_error())?;
        Ok(self.address(cell))
    }

    fn get_index_at(&self, row: i32, column: i32) -> fdo::Result<i32> {
        self.node
            .table_index_at(row, column)
            .map_err(self.map_error())
    }

    fn get_row_at_index(&self, index: i32) -> fdo::Result<i32> {
        self.node
            .table_row_at_index(index)
            .map_err(self.map_error())
    }

    fn get_column_at_index(&self, index: i32) -> fdo::Result<i32> {
        self.node
            .table_column_at_index(index)
            .map_err(self.map_error())
    }

    fn get_row_description(&self, row: i32) -> fdo::Result<String> {
        self.node
            .table_row_description(row)
            .map_err(self.map_error())
    }

    fn get_column_description(&self, column: i32) -> fdo::Result<String> {
        self.node
            .table_column_description(column)
            .map_err(self.map_error())
    }

    fn get_row_extent_at(&self, row: i32, column: i32) -> fdo::Result<i32> {
        self.node
            .table_row_extent_at(row, column)
            .map_err(self.map_error())
    }

    fn get_column_extent_at(&self, row: i32, column: i32) -> fdo::Result<i32> {
        self.node
            .table_column_extent_at(row, column)
            .map_err(self.map_error())
    }

    fn get_row_header(&self, row: i32) -> fdo::Result<(OwnedObjectAddress,)> {
        let header = self.node.table_row_header(row).map_err(self.map_error())?;
        Ok(self.address(header))
    }

    fn get_column_header(&self, column: i32) -> fdo::Result<(OwnedObjectAddress,)> {
        let header = self
            .node
            .table_column_header(column)
            .map_err(self.map_error())?;
        Ok(self.address(header))
    }

    fn get_selected_rows(&self) -> Vec<i32> {
        Vec::new()
    }

    fn get_selected_columns(&self) -> Vec<i32> {
        Vec::new()
    }

    fn is_row_selected(&self, _row: i32) -> bool {
        false
    }

    fn is_column_selected(&self, _column: i32) -> bool {
        false
    }

    fn is_selected(&self, _row: i32, _column: i32) -> bool {
        false
    }

    fn add_row_selection(&self, _row: i32) -> bool {
        false
    }

    fn add_column_selection(&self, _column: i32) -> bool {
        false
    }

    fn remove_row_selection(&self, _row: i32) -> bool {
        false
    }

    fn remove_column_selection(&self, _column: i32) -> bool {
        false
    }

    fn get_row_column_extents_at_index(
        &self,
        index: i32,
    ) -> fdo::Result<(bool, i32, i32, i32, i32, bool)> {
        self.node
            .table_row_column_extents_at_index(index)
            .map_err(self.map_error())
    }
}
