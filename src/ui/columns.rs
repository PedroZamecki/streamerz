//! Shared list and header geometry. The last column grows; narrow lists keep every column reachable.

/// Stable space for padding and the scrollbar, even when the list does not need to scroll.
pub(super) const INSET: i32 = 12;

#[derive(Clone, Copy)]
pub(super) struct Layout<'a> {
    widths: &'a [i32],
    count: usize,
    width: i32,
    preceding: i64,
    minimum: i64,
    weight: i64,
    fits: bool,
}

pub(super) fn layout(widths: &[i32], count: usize, widget_width: i32) -> Layout<'_> {
    let mut out = Layout {
        widths,
        count,
        width: widget_width.saturating_sub(2 * INSET).max(0),
        preceding: 0,
        minimum: 0,
        weight: 0,
        fits: true,
    };
    for i in 0..count {
        let nominal = out.nominal(i);
        let minimum = out.minimum_at(i);
        if i + 1 < count {
            out.preceding += nominal;
        }
        out.minimum += minimum;
        out.weight += nominal - minimum;
    }
    out.fits = count == 0 || out.preceding + out.minimum_at(count - 1) <= i64::from(out.width);
    out
}

impl Layout<'_> {
    fn nominal(&self, index: usize) -> i64 {
        i64::from(self.widths.get(index).copied().unwrap_or(160).max(1))
    }

    fn minimum_at(&self, index: usize) -> i64 {
        self.nominal(index).min(56)
    }

    fn boundary(&self, index: usize) -> i32 {
        if index >= self.count {
            return self.width;
        }
        if self.fits {
            return (0..index).map(|i| self.nominal(i)).sum::<i64>() as i32;
        }
        let minimum = (0..index).map(|i| self.minimum_at(i)).sum::<i64>();
        let width = i64::from(self.width);
        if width < self.minimum {
            // Even when useful text minima cannot fit, retain a positive target for every column
            // whenever there is at least one pixel per column. Full values remain on hover.
            let base = width.min(self.count as i64);
            let extra = width - base;
            let weights = self.minimum - self.count as i64;
            return (base * index as i64 / self.count as i64 + extra * (minimum - index as i64) / weights.max(1))
                as i32;
        }
        let weight = (0..index).map(|i| self.nominal(i) - self.minimum_at(i)).sum::<i64>();
        let extra = width - self.minimum;
        (minimum + extra * weight / self.weight.max(1)) as i32
    }

    pub(super) fn width(&self) -> i32 {
        self.width
    }

    /// A cell's offset from the shared left inset, and its allocated width.
    pub(super) fn cell(&self, index: usize) -> (i32, i32) {
        if index >= self.count {
            return (self.width, 0);
        }
        let start = self.boundary(index);
        (start, self.boundary(index + 1) - start)
    }

    /// Remaining text room after a leading image, cell gap, or disabled-state icon.
    pub(super) fn text_room(&self, index: usize, leading: i32, trailing: i32) -> i32 {
        self.cell(index)
            .1
            .saturating_sub(leading.max(0))
            .saturating_sub(trailing.max(0))
            .max(0)
    }

    /// Hit testing uses precisely the same boundaries as drawing, including the flexible last cell.
    pub(super) fn column_at(&self, x: i32) -> Option<usize> {
        if x < 0 || x >= self.width {
            return None;
        }
        (0..self.count).find(|&i| {
            let (start, width) = self.cell(i);
            (start..start + width).contains(&x)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_columns_fill_available_width_without_losing_last_column() {
        for nominal in [&[220, 420, 100, 200][..], &[220, 420, 100][..]] {
            for widget_width in 24..1000 {
                let columns = layout(nominal, 4, widget_width);
                let mut end = 0;
                for i in 0..4 {
                    let (start, width) = columns.cell(i);
                    assert_eq!(start, end);
                    assert!(width >= 0);
                    if columns.width() >= 4 {
                        assert!(width > 0);
                    }
                    end += width;
                    assert!(end <= columns.width());
                }
                assert_eq!(end, columns.width());
            }
        }
    }

    #[test]
    fn wide_columns_keep_nominal_preceding_widths_and_flex_last() {
        for nominal in [&[36, 220, 110, 110, 110][..], &[36, 220, 110, 110][..]] {
            let columns = layout(nominal, 5, 1100);
            assert_eq!(columns.cell(0), (0, 36));
            assert_eq!(columns.cell(1), (36, 220));
            assert_eq!(columns.cell(2), (256, 110));
            assert_eq!(columns.cell(3), (366, 110));
            assert_eq!(columns.cell(4), (476, 600));
        }
    }

    #[test]
    fn header_cells_and_pointer_targets_share_boundaries() {
        for width in [28, 60, 180, 340, 820, 1100] {
            let columns = layout(&[36, 220, 110], 4, width);
            for x in 0..columns.width() {
                let column = columns.column_at(x).unwrap();
                let (start, width) = columns.cell(column);
                assert!((start..start + width).contains(&x));
            }
            assert_eq!(columns.column_at(-1), None);
            assert_eq!(columns.column_at(columns.width()), None);
        }
    }

    #[test]
    fn disabled_icons_and_leading_images_cannot_make_text_room_negative() {
        for width in 0..600 {
            let columns = layout(&[36, 220], 3, width);
            assert!(columns.text_room(0, 24, 8) >= 0);
            assert!(columns.text_room(2, 0, 32) >= 0);
            assert!(columns.text_room(2, 0, 32) <= columns.cell(2).1);
        }
        let columns = layout(&[100], 2, 400);
        assert_eq!(columns.text_room(1, 0, 32), columns.cell(1).1 - 32);
    }

    #[test]
    fn empty_and_tiny_lists_are_safe() {
        assert_eq!(layout(&[], 0, 100).column_at(0), None);
        for width in [-10, 0, 10, 24] {
            let columns = layout(&[220], 2, width);
            assert_eq!(columns.width(), 0);
            assert_eq!(columns.cell(0), (0, 0));
            assert_eq!(columns.cell(1), (0, 0));
        }
    }
}
