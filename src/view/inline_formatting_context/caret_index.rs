use super::*;

/// Indexed equivalent of Parley's cluster-boundary cursor geometry. Build
/// visual positions once; repeated byte queries must not walk a run's entire
/// cluster prefix to find either the cluster or its visual offset.
/// The independent cursor comparisons in tests must also pass when upgrading
/// Parley; line lookup and soft-wrap affinity are part of this contract.
pub(super) struct CaretIndex {
    visual: Vec<ClusterGeometry>,
    byte_to_visual: Vec<usize>,
    text_len: usize,
    last_line: InlineIfcPaintRect,
}

struct LineIndex {
    bytes: Range<usize>,
    logical: Range<usize>,
}

struct ClusterGeometry {
    bytes: Range<usize>,
    rect: InlineIfcPaintRect,
    rtl: bool,
    line_end: bool,
    soft_break: bool,
    hard_break: bool,
}

impl CaretIndex {
    pub(super) fn new(layout: &ParleyLayout<[u8; 4]>, text_len: usize) -> Self {
        let mut visual = Vec::new();
        let mut logical = Vec::new();
        let mut lines = Vec::new();
        for line in layout.lines() {
            let first_cluster = visual.len();
            let metrics = line.metrics();
            for run in line.runs() {
                let mut clusters = run.visual_clusters().peekable();
                // One offset lookup per run accounts for preceding inline
                // boxes and alignment. Within the run use the same ordered
                // f32 advance additions as Parley's visual_offset().
                let mut x = clusters
                    .peek()
                    .and_then(|c| c.visual_offset())
                    .unwrap_or_default();
                for cluster in clusters {
                    let advance = cluster.advance();
                    visual.push(ClusterGeometry {
                        bytes: cluster.text_range(),
                        rect: InlineIfcPaintRect {
                            x,
                            y: metrics.block_min_coord,
                            width: advance,
                            height: metrics.block_max_coord - metrics.block_min_coord,
                        },
                        rtl: cluster.is_rtl(),
                        line_end: cluster.is_end_of_line(),
                        soft_break: cluster.is_soft_line_break(),
                        hard_break: cluster.is_hard_line_break(),
                    });
                    x += advance;
                }
            }
            let first_logical = logical.len();
            logical.extend(first_cluster..visual.len());
            logical[first_logical..].sort_by_key(|&i| visual[i].bytes.start);
            lines.push(LineIndex {
                bytes: line.text_range(),
                logical: first_logical..logical.len(),
            });
        }
        let last_line = layout
            .get(layout.len().saturating_sub(1))
            .map(|line| {
                let m = line.metrics();
                InlineIfcPaintRect {
                    x: m.offset,
                    y: m.block_min_coord,
                    width: 0.0,
                    height: m.block_max_coord - m.block_min_coord,
                }
            })
            .unwrap_or(InlineIfcPaintRect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            });
        // Queries ask for both affinities at nearly every cluster boundary.
        // Resolve the byte authority once, rather than repeating two binary
        // searches for each of the cursor's neighboring cluster lookups.
        let byte_to_visual = (0..text_len)
            .map(|byte| Self::find_cluster(&visual, &logical, &lines, byte).unwrap_or(usize::MAX))
            .collect();
        Self {
            visual,
            byte_to_visual,
            text_len,
            last_line,
        }
    }

    fn find_cluster(
        visual: &[ClusterGeometry],
        logical: &[usize],
        lines: &[LineIndex],
        byte: usize,
    ) -> Option<usize> {
        // Preserve Parley's line-range lookup before considering clusters.
        // Atomic-only lines can share insertion boundaries with adjacent text;
        // searching all clusters globally changes cursor normalization there.
        let line = lines
            .binary_search_by(|line| {
                if byte < line.bytes.start {
                    std::cmp::Ordering::Greater
                } else if byte >= line.bytes.end {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .ok()?;
        let logical = &logical[lines[line].logical.clone()];
        let position = logical
            .partition_point(|&i| visual[i].bytes.start <= byte)
            .checked_sub(1)?;
        let index = logical[position];
        visual[index].bytes.contains(&byte).then_some(index)
    }

    fn cluster_at(&self, byte: usize) -> Option<usize> {
        self.byte_to_visual
            .get(byte)
            .copied()
            .filter(|&i| i != usize::MAX)
    }

    fn previous(&self, index: usize) -> Option<usize> {
        index.checked_sub(1)
    }

    fn next(&self, index: usize) -> Option<usize> {
        (index + 1 < self.visual.len()).then_some(index + 1)
    }

    pub(super) fn rect(&self, byte: usize, affinity: InlineIfcCaretAffinity) -> InlineIfcPaintRect {
        // Cursor::from_byte_index normalizes to a cluster start and forces
        // downstream at byte zero, upstream past the final cluster.
        let (byte, affinity) = match self.cluster_at(byte) {
            Some(index) => {
                let start = self.visual[index].bytes.start;
                (
                    start,
                    if start == 0 {
                        InlineIfcCaretAffinity::Downstream
                    } else {
                        affinity
                    },
                )
            }
            None => (self.text_len, InlineIfcCaretAffinity::Upstream),
        };
        let upstream = byte.checked_sub(1).and_then(|b| self.cluster_at(b));
        let downstream = self.cluster_at(byte);
        let (left, right) = match affinity {
            InlineIfcCaretAffinity::Upstream => match upstream {
                Some(i) if self.visual[i].rtl => (self.previous(i), Some(i)),
                Some(i) => (Some(i), self.next(i)),
                None => match downstream {
                    Some(i) if self.visual[i].rtl => (None, Some(i)),
                    Some(i) => (Some(i), None),
                    None => (None, None),
                },
            },
            InlineIfcCaretAffinity::Downstream => match downstream {
                Some(i) if self.visual[i].rtl => (Some(i), self.next(i)),
                Some(i) => (self.previous(i), Some(i)),
                None => match upstream {
                    Some(i) if self.visual[i].rtl => (None, Some(i)),
                    Some(i) => (Some(i), None),
                    None => (None, None),
                },
            },
        };
        // Resolve the visual edge, retaining Parley's special treatment of
        // soft wraps, mandatory breaks and the empty trailing line.
        let (index, at_end) = match (left, right) {
            (Some(l), Some(r)) if self.visual[l].line_end => {
                let preceding = &self.visual[l];
                if preceding.soft_break
                    && (preceding.rtl == (affinity == InlineIfcCaretAffinity::Downstream))
                {
                    (l, true)
                } else {
                    (r, false)
                }
            }
            (Some(l), None) if self.visual[l].hard_break => return self.last_line,
            (Some(l), _) => (l, true),
            (None, Some(r)) => (r, false),
            (None, None) => return self.last_line,
        };
        let mut rect = self.visual[index].rect;
        if at_end {
            rect.x += rect.width;
        }
        rect.width = 0.0;
        rect
    }
}
