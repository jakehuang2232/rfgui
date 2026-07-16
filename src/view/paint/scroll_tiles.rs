//! Pure, graph-inert scroll-content tile geometry.
//!
//! This scaffold deliberately remains at DPR 1. Production admission must not
//! use it until physical-pixel quantisation exists for other scale factors.

#![allow(dead_code)] // B1-A is graph-inert; B1-B wires the production consumer.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ScrollContentTileIndex {
    pub(crate) row: u32,
    pub(crate) column: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScrollContentTileBounds {
    /// Non-overlapping logical pixels assigned to this tile.
    pub(crate) interior: [u32; 4],
    /// Raster allocation including the sampling gutter, clamped to content.
    pub(crate) raster: [u32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScrollContentTileRasterIdentity {
    pub(crate) index: ScrollContentTileIndex,
    pub(crate) content_bounds: [u32; 4],
    pub(crate) bounds: ScrollContentTileBounds,
    pub(crate) tile_edge: u32,
    pub(crate) gutter: u32,
}

impl ScrollContentTileRasterIdentity {
    pub(crate) fn new(
        index: ScrollContentTileIndex,
        content_bounds: [u32; 4],
        bounds: ScrollContentTileBounds,
        tile_edge: u32,
        gutter: u32,
    ) -> Option<Self> {
        bounds
            .is_canonical_for(content_bounds, tile_edge, gutter, index)
            .then_some(Self {
                index,
                content_bounds,
                bounds,
                tile_edge,
                gutter,
            })
    }

    pub(crate) fn is_canonical(self) -> bool {
        self.bounds
            .is_canonical_for(self.content_bounds, self.tile_edge, self.gutter, self.index)
    }
}

impl ScrollContentTileBounds {
    pub(crate) fn for_index(
        content_bounds: [u32; 4],
        tile_edge: u32,
        gutter: u32,
        index: ScrollContentTileIndex,
    ) -> Option<Self> {
        let [content_x, content_y, content_width, content_height] = content_bounds;
        if content_width == 0 || content_height == 0 || tile_edge == 0 {
            return None;
        }
        let content_right = content_x.checked_add(content_width)?;
        let content_bottom = content_y.checked_add(content_height)?;
        let local_x = index.column.checked_mul(tile_edge)?;
        let local_y = index.row.checked_mul(tile_edge)?;
        if local_x >= content_width || local_y >= content_height {
            return None;
        }
        let interior_x = content_x.checked_add(local_x)?;
        let interior_y = content_y.checked_add(local_y)?;
        let interior_width = tile_edge.min(content_width - local_x);
        let interior_height = tile_edge.min(content_height - local_y);
        let interior_right = interior_x.checked_add(interior_width)?;
        let interior_bottom = interior_y.checked_add(interior_height)?;
        let raster_x = interior_x.saturating_sub(gutter).max(content_x);
        let raster_y = interior_y.saturating_sub(gutter).max(content_y);
        let raster_right = interior_right.saturating_add(gutter).min(content_right);
        let raster_bottom = interior_bottom.saturating_add(gutter).min(content_bottom);
        Some(Self {
            interior: [interior_x, interior_y, interior_width, interior_height],
            raster: [
                raster_x,
                raster_y,
                raster_right.checked_sub(raster_x)?,
                raster_bottom.checked_sub(raster_y)?,
            ],
        })
    }

    pub(crate) fn is_canonical_for(
        self,
        content_bounds: [u32; 4],
        tile_edge: u32,
        gutter: u32,
        index: ScrollContentTileIndex,
    ) -> bool {
        Self::for_index(content_bounds, tile_edge, gutter, index) == Some(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScrollContentActiveTileManifest {
    content_bounds: [u32; 4],
    tile_edge: u32,
    gutter: u32,
    overscan: u32,
    tiles: Vec<(ScrollContentTileIndex, ScrollContentTileBounds)>,
}

impl ScrollContentActiveTileManifest {
    pub(crate) fn content_bounds(&self) -> [u32; 4] {
        self.content_bounds
    }

    pub(crate) fn tile_edge(&self) -> u32 {
        self.tile_edge
    }

    pub(crate) fn gutter(&self) -> u32 {
        self.gutter
    }

    pub(crate) fn overscan(&self) -> u32 {
        self.overscan
    }

    pub(crate) fn tiles(&self) -> &[(ScrollContentTileIndex, ScrollContentTileBounds)] {
        &self.tiles
    }
}

/// Sealed exact active-set authority for one future tiled scroll-scene
/// transaction. Callers cannot supply indices independently of the validated
/// row-major planner output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScrollContentTileSetTransactionStamp {
    content_root: crate::view::node_arena::NodeKey,
    content_stable_id: u64,
    content_bounds: [u32; 4],
    tile_edge: u32,
    gutter: u32,
    overscan: u32,
    indices: Vec<ScrollContentTileIndex>,
}

impl ScrollContentTileSetTransactionStamp {
    pub(crate) fn from_active_manifest(
        content_root: crate::view::node_arena::NodeKey,
        content_stable_id: u64,
        manifest: &ScrollContentActiveTileManifest,
    ) -> Option<Self> {
        if content_stable_id == 0
            || manifest.tile_edge == 0
            || manifest.gutter != 1
            || manifest.tiles.is_empty()
        {
            return None;
        }
        let mut indices = Vec::with_capacity(manifest.tiles.len());
        let mut previous = None;
        for &(index, bounds) in &manifest.tiles {
            if !bounds.is_canonical_for(
                manifest.content_bounds,
                manifest.tile_edge,
                manifest.gutter,
                index,
            ) || previous.is_some_and(|previous| previous >= index)
            {
                return None;
            }
            previous = Some(index);
            indices.push(index);
        }
        let first = *indices.first()?;
        let last = *indices.last()?;
        let first_row_width = indices
            .iter()
            .take_while(|index| index.row == first.row)
            .count();
        let column_count = u32::try_from(first_row_width).ok()?;
        let row_count = last.row.checked_sub(first.row)?.checked_add(1)?;
        let expected_len = usize::try_from(column_count.checked_mul(row_count)?).ok()?;
        if column_count == 0 || expected_len != indices.len() {
            return None;
        }
        for (ordinal, &actual) in indices.iter().enumerate() {
            let ordinal = u32::try_from(ordinal).ok()?;
            let expected = ScrollContentTileIndex {
                row: first.row.checked_add(ordinal / column_count)?,
                column: first.column.checked_add(ordinal % column_count)?,
            };
            if actual != expected {
                return None;
            }
        }
        Some(Self {
            content_root,
            content_stable_id,
            content_bounds: manifest.content_bounds,
            tile_edge: manifest.tile_edge,
            gutter: manifest.gutter,
            overscan: manifest.overscan,
            indices,
        })
    }

    pub(crate) fn content_root(&self) -> crate::view::node_arena::NodeKey {
        self.content_root
    }

    pub(crate) fn content_stable_id(&self) -> u64 {
        self.content_stable_id
    }

    pub(crate) fn content_bounds(&self) -> [u32; 4] {
        self.content_bounds
    }

    pub(crate) fn tile_edge(&self) -> u32 {
        self.tile_edge
    }

    pub(crate) fn gutter(&self) -> u32 {
        self.gutter
    }

    pub(crate) fn overscan(&self) -> u32 {
        self.overscan
    }

    pub(crate) fn indices(&self) -> &[ScrollContentTileIndex] {
        &self.indices
    }
}

/// Selects the exact row-major tile set intersecting the current scrollport
/// translated into offset-zero content coordinates plus logical overscan.
pub(crate) fn plan_active_scroll_content_tiles_dpr1(
    content_bounds: [u32; 4],
    offset: [f32; 2],
    contents_clip: [u32; 4],
    tile_edge: u32,
    gutter: u32,
    overscan: u32,
) -> Option<ScrollContentActiveTileManifest> {
    let [content_x, content_y, content_width, content_height] = content_bounds;
    let [clip_x, clip_y, clip_width, clip_height] = contents_clip;
    let content_right = content_x.checked_add(content_width)?;
    let content_bottom = content_y.checked_add(content_height)?;
    clip_x.checked_add(clip_width)?;
    clip_y.checked_add(clip_height)?;
    if content_width == 0
        || content_height == 0
        || clip_width == 0
        || clip_height == 0
        || tile_edge == 0
        || offset.iter().any(|value| !value.is_finite())
    {
        return None;
    }

    let overscan = f64::from(overscan);
    let source_left =
        (f64::from(clip_x) + f64::from(offset[0]) - overscan).max(f64::from(content_x));
    let source_top =
        (f64::from(clip_y) + f64::from(offset[1]) - overscan).max(f64::from(content_y));
    let source_right =
        (f64::from(clip_x) + f64::from(clip_width) + f64::from(offset[0]) + overscan)
            .min(f64::from(content_right));
    let source_bottom =
        (f64::from(clip_y) + f64::from(clip_height) + f64::from(offset[1]) + overscan)
            .min(f64::from(content_bottom));

    let mut tiles = Vec::new();
    if source_left < source_right && source_top < source_bottom {
        let edge = f64::from(tile_edge);
        let first_column = ((source_left - f64::from(content_x)) / edge).floor() as u32;
        let first_row = ((source_top - f64::from(content_y)) / edge).floor() as u32;
        let column_end = ((source_right - f64::from(content_x)) / edge).ceil() as u32;
        let row_end = ((source_bottom - f64::from(content_y)) / edge).ceil() as u32;
        for row in first_row..row_end {
            for column in first_column..column_end {
                let index = ScrollContentTileIndex { column, row };
                let bounds =
                    ScrollContentTileBounds::for_index(content_bounds, tile_edge, gutter, index)?;
                tiles.push((index, bounds));
            }
        }
    }

    Some(ScrollContentActiveTileManifest {
        content_bounds,
        tile_edge,
        gutter,
        overscan: overscan as u32,
        tiles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odd_content_is_covered_once_in_row_major_order_with_clamped_gutters() {
        let manifest = plan_active_scroll_content_tiles_dpr1(
            [10, 20, 2051, 1027],
            [0.0, 0.0],
            [10, 20, 2051, 1027],
            1024,
            1,
            0,
        )
        .unwrap();
        let indices = manifest
            .tiles
            .iter()
            .map(|(index, _)| *index)
            .collect::<Vec<_>>();
        assert_eq!(
            indices,
            vec![
                ScrollContentTileIndex { column: 0, row: 0 },
                ScrollContentTileIndex { column: 1, row: 0 },
                ScrollContentTileIndex { column: 2, row: 0 },
                ScrollContentTileIndex { column: 0, row: 1 },
                ScrollContentTileIndex { column: 1, row: 1 },
                ScrollContentTileIndex { column: 2, row: 1 },
            ]
        );
        assert_eq!(manifest.tiles[0].1.interior, [10, 20, 1024, 1024]);
        assert_eq!(manifest.tiles[0].1.raster, [10, 20, 1025, 1025]);
        assert_eq!(manifest.tiles[1].1.interior, [1034, 20, 1024, 1024]);
        assert_eq!(manifest.tiles[1].1.raster, [1033, 20, 1026, 1025]);
        assert_eq!(manifest.tiles[5].1.interior, [2058, 1044, 3, 3]);
        assert_eq!(manifest.tiles[5].1.raster, [2057, 1043, 4, 4]);
        let area = manifest
            .tiles
            .iter()
            .map(|(_, bounds)| u64::from(bounds.interior[2]) * u64::from(bounds.interior[3]))
            .sum::<u64>();
        assert_eq!(area, 2051_u64 * 1027_u64);
    }

    #[test]
    fn full_two_dimensional_offset_and_overscan_select_row_major_tiles() {
        let manifest = plan_active_scroll_content_tiles_dpr1(
            [100, 200, 1600, 1600],
            [550.5, 620.25],
            [100, 200, 300, 300],
            512,
            1,
            64,
        )
        .unwrap();
        assert_eq!(
            manifest
                .tiles
                .iter()
                .map(|(index, _)| *index)
                .collect::<Vec<_>>(),
            vec![
                ScrollContentTileIndex { column: 0, row: 1 },
                ScrollContentTileIndex { column: 1, row: 1 },
            ]
        );
    }

    #[test]
    fn invalid_or_overflowing_geometry_fails_closed() {
        assert!(
            plan_active_scroll_content_tiles_dpr1(
                [u32::MAX, 0, 2, 2],
                [0.0, 0.0],
                [0, 0, 1, 1],
                32,
                1,
                0,
            )
            .is_none()
        );
        assert!(
            plan_active_scroll_content_tiles_dpr1(
                [0, 0, 10, 10],
                [f32::NAN, 0.0],
                [0, 0, 1, 1],
                32,
                1,
                0,
            )
            .is_none()
        );
    }

    #[test]
    fn transaction_stamp_accepts_only_exact_row_major_manifest() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let root = slots.insert(());
        let manifest = plan_active_scroll_content_tiles_dpr1(
            [0, 0, 300, 900],
            [0.0, 0.0],
            [0, 0, 100, 300],
            128,
            1,
            0,
        )
        .unwrap();
        let token =
            ScrollContentTileSetTransactionStamp::from_active_manifest(root, 7001, &manifest)
                .unwrap();
        assert_eq!(token.indices().len(), 3);
        assert_eq!(
            token.indices()[0],
            ScrollContentTileIndex { row: 0, column: 0 }
        );
        assert_eq!(
            token.indices()[2],
            ScrollContentTileIndex { row: 2, column: 0 }
        );

        let mut reversed = manifest.clone();
        reversed.tiles.reverse();
        assert!(
            ScrollContentTileSetTransactionStamp::from_active_manifest(root, 7001, &reversed,)
                .is_none()
        );

        let mut duplicate = manifest.clone();
        duplicate.tiles[1] = duplicate.tiles[0];
        assert!(
            ScrollContentTileSetTransactionStamp::from_active_manifest(root, 7001, &duplicate,)
                .is_none()
        );

        let mut missing_middle_row = manifest.clone();
        missing_middle_row.tiles.remove(1);
        assert!(
            ScrollContentTileSetTransactionStamp::from_active_manifest(
                root,
                7001,
                &missing_middle_row,
            )
            .is_none()
        );

        let mut sparse_columns = plan_active_scroll_content_tiles_dpr1(
            [0, 0, 900, 300],
            [0.0, 0.0],
            [0, 0, 300, 100],
            128,
            1,
            0,
        )
        .unwrap();
        assert_eq!(sparse_columns.tiles.len(), 3);
        sparse_columns.tiles.remove(1);
        assert!(
            ScrollContentTileSetTransactionStamp::from_active_manifest(
                root,
                7001,
                &sparse_columns,
            )
            .is_none()
        );

        let mut clone_tamper = manifest.clone();
        clone_tamper.tile_edge = 64;
        assert!(
            ScrollContentTileSetTransactionStamp::from_active_manifest(root, 7001, &clone_tamper,)
                .is_none()
        );

        let mut wrong_bounds = manifest;
        wrong_bounds.tiles[0].1.raster[2] += 1;
        assert!(
            ScrollContentTileSetTransactionStamp::from_active_manifest(root, 7001, &wrong_bounds,)
                .is_none()
        );
    }
}
