//! Rust side of the `Select` control (ui/components/controls.slint): its options model, and
//! its `current-index` as a position in the Rust list behind those options.

use slint::{ModelRc, SharedString, VecModel};

/// A `Select`'s `options`, from their labels.
pub fn options<'a>(labels: impl Iterator<Item = &'a str>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        labels.map(SharedString::from).collect::<Vec<_>>(),
    ))
}

/// `current-index` clamped into a list of `len` options (0 for an empty list), so a stale or
/// negative index never panics.
pub fn index(i: i32, len: usize) -> usize {
    usize::try_from(i).unwrap_or(0).min(len.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_is_clamped() {
        assert_eq!(index(-1, 3), 0);
        assert_eq!(index(7, 3), 2);
        assert_eq!(index(0, 0), 0);
    }
}
