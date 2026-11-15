// Copyright 2026 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

//! Table geometry for the AT-SPI `Table` and `TableCell` interfaces.
//!
//! The model is computed from the AccessKit tree (not the filtered AT-SPI
//! tree), so cells may be nested under rows, row groups or generic
//! containers. Explicit `row_index`/`column_index` values win; missing
//! indices are derived HTML-style: rows in tree order, cells left to right
//! skipping slots already covered by a row or column span from above.

use accesskit::Role;
use accesskit_consumer::{FilterResult, FullNodeId, NodeRef};

use crate::filters::filter;

// HTML caps colspan at 1000 and rowspan at 65534; cap the slots we mark
// while deriving positions so a bogus span cannot exhaust memory.
const MAX_DERIVED_SPAN: usize = 1000;

pub(crate) fn is_table_role(role: Role) -> bool {
    matches!(
        role,
        Role::Table | Role::Grid | Role::ListGrid | Role::TreeGrid
    )
}

pub(crate) fn is_table_cell_role(role: Role) -> bool {
    matches!(
        role,
        Role::Cell | Role::GridCell | Role::ColumnHeader | Role::RowHeader
    )
}

/// The nearest table ancestor of a cell in the AccessKit tree.
pub(crate) fn containing_table<'a>(cell: &NodeRef<'a>) -> Option<NodeRef<'a>> {
    let mut current = cell.parent()?;
    loop {
        if is_table_role(current.role()) {
            return Some(current);
        }
        current = current.parent()?;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TableCellInfo {
    pub(crate) id: FullNodeId,
    pub(crate) role: Role,
    pub(crate) row: usize,
    pub(crate) column: usize,
    pub(crate) row_span: usize,
    pub(crate) column_span: usize,
}

impl TableCellInfo {
    pub(crate) fn covers_row(&self, row: usize) -> bool {
        row >= self.row && row - self.row < self.row_span
    }

    pub(crate) fn covers_column(&self, column: usize) -> bool {
        column >= self.column && column - self.column < self.column_span
    }

    pub(crate) fn covers(&self, row: usize, column: usize) -> bool {
        self.covers_row(row) && self.covers_column(column)
    }

    fn overlaps_rows(&self, other: &TableCellInfo) -> bool {
        self.row < other.row + other.row_span && other.row < self.row + self.row_span
    }

    fn overlaps_columns(&self, other: &TableCellInfo) -> bool {
        self.column < other.column + other.column_span
            && other.column < self.column + self.column_span
    }
}

pub(crate) struct TableModel {
    pub(crate) row_count: usize,
    pub(crate) column_count: usize,
    pub(crate) caption: Option<FullNodeId>,
    /// Cells in tree order.
    pub(crate) cells: Vec<TableCellInfo>,
}

struct RowEntry<'a> {
    row_index: Option<usize>,
    cells: Vec<NodeRef<'a>>,
}

fn collect<'a>(
    node: &NodeRef<'a>,
    current_row: Option<usize>,
    rows: &mut Vec<RowEntry<'a>>,
    caption: &mut Option<FullNodeId>,
) {
    for child in node.children() {
        if filter(&child) == FilterResult::ExcludeSubtree {
            continue;
        }
        let role = child.role();
        if is_table_role(role) {
            // Nested tables own their own cells.
            continue;
        }
        if role == Role::Row {
            rows.push(RowEntry {
                row_index: child.data().row_index(),
                cells: Vec::new(),
            });
            let index = rows.len() - 1;
            collect(&child, Some(index), rows, caption);
            continue;
        }
        if is_table_cell_role(role) {
            match current_row {
                Some(index) => rows[index].cells.push(child),
                // A cell outside any row forms an implicit row of its own.
                None => rows.push(RowEntry {
                    row_index: None,
                    cells: vec![child],
                }),
            }
            continue;
        }
        if role == Role::Caption && caption.is_none() && filter(&child) == FilterResult::Include {
            *caption = Some(child.id());
            continue;
        }
        collect(&child, current_row, rows, caption);
    }
}

fn is_occupied(occupied: &[Vec<bool>], row: usize, column: usize) -> bool {
    occupied
        .get(row)
        .and_then(|columns| columns.get(column))
        .copied()
        .unwrap_or(false)
}

fn mark_occupied(occupied: &mut Vec<Vec<bool>>, cell: &TableCellInfo) {
    let row_end = cell.row + cell.row_span.min(MAX_DERIVED_SPAN);
    let column_end = cell.column + cell.column_span.min(MAX_DERIVED_SPAN);
    if occupied.len() < row_end {
        occupied.resize_with(row_end, Vec::new);
    }
    for columns in &mut occupied[cell.row..row_end] {
        if columns.len() < column_end {
            columns.resize(column_end, false);
        }
        columns[cell.column..column_end].fill(true);
    }
}

impl TableModel {
    pub(crate) fn new(table: &NodeRef) -> Self {
        let mut rows = Vec::new();
        let mut caption = None;
        collect(table, None, &mut rows, &mut caption);

        let mut cells = Vec::new();
        let mut occupied: Vec<Vec<bool>> = Vec::new();
        let mut next_row = 0;
        let mut derived_rows = 0;
        let mut derived_columns = 0;
        for entry in rows {
            let row_index = entry.row_index.unwrap_or(next_row);
            next_row = row_index + 1;
            derived_rows = derived_rows.max(next_row);
            let mut cursor = 0;
            for cell in entry.cells {
                let data = cell.data();
                let row = data.row_index().unwrap_or(row_index);
                let column = data.column_index().unwrap_or_else(|| {
                    let mut column = cursor;
                    while is_occupied(&occupied, row, column) {
                        column += 1;
                    }
                    column
                });
                let info = TableCellInfo {
                    id: cell.id(),
                    role: cell.role(),
                    row,
                    column,
                    row_span: data.row_span().unwrap_or(1).max(1),
                    column_span: data.column_span().unwrap_or(1).max(1),
                };
                mark_occupied(&mut occupied, &info);
                cursor = column.saturating_add(info.column_span);
                derived_rows = derived_rows.max(row.saturating_add(info.row_span));
                derived_columns = derived_columns.max(cursor);
                cells.push(info);
            }
        }

        let data = table.data();
        Self {
            row_count: data.row_count().unwrap_or(0).max(derived_rows),
            column_count: data.column_count().unwrap_or(0).max(derived_columns),
            caption,
            cells,
        }
    }

    pub(crate) fn cell(&self, id: FullNodeId) -> Option<&TableCellInfo> {
        self.cells.iter().find(|cell| cell.id == id)
    }

    pub(crate) fn cell_at(&self, row: usize, column: usize) -> Option<&TableCellInfo> {
        self.cells.iter().find(|cell| cell.covers(row, column))
    }

    pub(crate) fn index_to_position(&self, index: usize) -> Option<(usize, usize)> {
        if self.column_count == 0 || index / self.column_count >= self.row_count {
            return None;
        }
        Some((index / self.column_count, index % self.column_count))
    }

    pub(crate) fn column_header(&self, column: usize) -> Option<&TableCellInfo> {
        self.cells
            .iter()
            .filter(|cell| cell.role == Role::ColumnHeader && cell.covers_column(column))
            .min_by_key(|cell| cell.row)
    }

    pub(crate) fn row_header(&self, row: usize) -> Option<&TableCellInfo> {
        self.cells
            .iter()
            .filter(|cell| cell.role == Role::RowHeader && cell.covers_row(row))
            .min_by_key(|cell| cell.column)
    }

    /// Column headers above `cell` that share at least one of its columns.
    pub(crate) fn column_header_cells<'a>(
        &'a self,
        cell: &'a TableCellInfo,
    ) -> impl Iterator<Item = &'a TableCellInfo> + 'a {
        self.cells.iter().filter(move |header| {
            header.role == Role::ColumnHeader
                && header.id != cell.id
                && header.row < cell.row
                && header.overlaps_columns(cell)
        })
    }

    /// Row headers left of `cell` that share at least one of its rows.
    pub(crate) fn row_header_cells<'a>(
        &'a self,
        cell: &'a TableCellInfo,
    ) -> impl Iterator<Item = &'a TableCellInfo> + 'a {
        self.cells.iter().filter(move |header| {
            header.role == Role::RowHeader
                && header.id != cell.id
                && header.column < cell.column
                && header.overlaps_rows(cell)
        })
    }
}

#[cfg(test)]
mod tests {
    use accesskit::{Node, NodeId, Role};
    use atspi_common::Interface;

    use crate::{
        Adapter, PlatformNode,
        test_util::{adapter, node_at, with_children},
    };

    fn labelled(role: Role, label: &str) -> Node {
        let mut node = Node::new(role);
        node.set_label(label);
        node
    }

    // | Name  | Size |
    // | a.txt | 1 KB |
    // | Total (2 columns) |
    fn table() -> Adapter {
        let mut files = labelled(Role::Table, "Files");
        files.set_children(vec![NodeId(2), NodeId(5), NodeId(8)]);
        let mut total = labelled(Role::Cell, "Total");
        total.set_column_span(2);
        adapter(vec![
            (NodeId(0), with_children(Role::Window, &[NodeId(1)])),
            (NodeId(1), files),
            (NodeId(2), with_children(Role::Row, &[NodeId(3), NodeId(4)])),
            (NodeId(3), labelled(Role::ColumnHeader, "Name")),
            (NodeId(4), labelled(Role::ColumnHeader, "Size")),
            (NodeId(5), with_children(Role::Row, &[NodeId(6), NodeId(7)])),
            (NodeId(6), labelled(Role::Cell, "a.txt")),
            (NodeId(7), labelled(Role::Cell, "1 KB")),
            (NodeId(8), with_children(Role::Row, &[NodeId(9)])),
            (NodeId(9), total),
        ])
    }

    fn name(a: &Adapter, id: Option<accesskit_consumer::FullNodeId>) -> String {
        a.platform_node(id.unwrap()).name().unwrap()
    }

    fn cell(a: &Adapter, row: usize, column: usize) -> PlatformNode {
        node_at(a, &[0, row, column])
    }

    #[test]
    fn table_geometry_and_headers() {
        let a = table();
        let table = node_at(&a, &[0]);
        assert!(table.interfaces().unwrap().contains(Interface::Table));
        assert_eq!(table.table_row_count().unwrap(), 3);
        assert_eq!(table.table_column_count().unwrap(), 2);
        assert_eq!(name(&a, table.table_cell_at(1, 0).unwrap()), "a.txt");
        // The spanning cell covers both columns of the last row.
        assert_eq!(name(&a, table.table_cell_at(2, 1).unwrap()), "Total");
        assert_eq!(name(&a, table.table_column_header(1).unwrap()), "Size");
        assert_eq!(table.table_cell_at(3, 0).unwrap(), None);
        assert_eq!(table.table_index_at(1, 1).unwrap(), 3);
        assert_eq!(table.table_index_at(3, 0).unwrap(), -1);
    }

    #[test]
    fn table_cell_position_span_and_headers() {
        let a = table();
        let size = cell(&a, 1, 1);
        assert!(size.interfaces().unwrap().contains(Interface::TableCell));
        assert_eq!(size.table_cell_position().unwrap(), (1, 1));
        let headers: Vec<String> = size
            .table_cell_column_header_cells()
            .unwrap()
            .into_iter()
            .map(|id| name(&a, Some(id)))
            .collect();
        assert_eq!(headers, ["Size"]);
        assert_eq!(name(&a, size.table_cell_table().unwrap()), "Files");

        let total = cell(&a, 2, 0);
        assert_eq!(total.table_cell_row_column_span().unwrap(), (2, 0, 1, 2));

        // Rows are neither tables nor cells.
        let row = node_at(&a, &[0, 0]);
        let interfaces = row.interfaces().unwrap();
        assert!(!interfaces.contains(Interface::Table));
        assert!(!interfaces.contains(Interface::TableCell));
    }
}
