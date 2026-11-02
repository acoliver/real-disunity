//! Signed distance from a bitmap's boundary, by the 8-point sequential Euclidean distance
//! transform (8SSEDT): two sweeps over the grid propagate, to each cell, the offset to the nearest
//! seed cell found so far through the cell's already-visited neighbours.

/// An offset larger than any grid, so an unreached cell never wins a comparison.
const FAR: i32 = 10_000;

#[derive(Clone, Copy)]
struct Offset {
    dx: i32,
    dy: i32,
}
impl Offset {
    const SEED: Self = Self { dx: 0, dy: 0 };
    const UNREACHED: Self = Self { dx: FAR, dy: FAR };
    fn squared(self) -> i32 {
        self.dx * self.dx + self.dy * self.dy
    }
}

struct Grid {
    cells: Vec<Offset>,
    width: usize,
    height: usize,
}
impl Grid {
    fn new(seeds: impl Iterator<Item = bool>, width: usize, height: usize) -> Self {
        let cells = seeds
            .map(|seed| {
                if seed {
                    Offset::SEED
                } else {
                    Offset::UNREACHED
                }
            })
            .collect();
        Self {
            cells,
            width,
            height,
        }
    }

    /// Takes the offset through the neighbour at `(x + ox, y + oy)` when it is nearer.
    fn compare(&mut self, x: usize, y: usize, ox: isize, oy: isize) {
        let (Some(nx), Some(ny)) = (x.checked_add_signed(ox), y.checked_add_signed(oy)) else {
            return;
        };
        if nx >= self.width || ny >= self.height {
            return;
        }
        let neighbour = self.cells[ny * self.width + nx];
        let through = Offset {
            dx: neighbour.dx + i32::try_from(ox).unwrap_or(0),
            dy: neighbour.dy + i32::try_from(oy).unwrap_or(0),
        };
        let cell = &mut self.cells[y * self.width + x];
        if through.squared() < cell.squared() {
            *cell = through;
        }
    }

    fn sweep_down(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                for (ox, oy) in [(-1, 0), (0, -1), (-1, -1), (1, -1)] {
                    self.compare(x, y, ox, oy);
                }
            }
            for x in (0..self.width).rev() {
                self.compare(x, y, 1, 0);
            }
        }
    }

    fn sweep_up(&mut self) {
        for y in (0..self.height).rev() {
            for x in (0..self.width).rev() {
                for (ox, oy) in [(1, 0), (0, 1), (-1, 1), (1, 1)] {
                    self.compare(x, y, ox, oy);
                }
            }
            for x in 0..self.width {
                self.compare(x, y, -1, 0);
            }
        }
    }

    /// Distance from each cell's centre to the nearest seed cell's centre, in cells.
    fn distances(mut self) -> Vec<f64> {
        self.sweep_down();
        self.sweep_up();
        self.cells
            .iter()
            .map(|cell| f64::from(cell.squared()).sqrt())
            .collect()
    }
}

/// The signed distance from each cell's centre to the edge between inside and outside cells, in
/// cells: positive inside, negative outside. The edge lies half a cell from the nearest centre of
/// the other kind. A bitmap with no cell of one kind has no edge, and its distances are huge.
pub(super) fn signed_distance(inside: &[bool], width: usize, height: usize) -> Vec<f64> {
    let to_outside = Grid::new(inside.iter().map(|&i| !i), width, height).distances();
    let to_inside = Grid::new(inside.iter().copied(), width, height).distances();
    inside
        .iter()
        .zip(to_outside.into_iter().zip(to_inside))
        .map(|(&i, (out, inn))| if i { out - 0.5 } else { 0.5 - inn })
        .collect()
}
