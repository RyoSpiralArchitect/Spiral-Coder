//! Repeated role slices spend one shared cap, including compatibility retries.
pub(super) fn allowance(
    remaining: usize,
    reserve: usize,
    pending: usize,
    limit: Option<usize>,
) -> usize {
    // Small default sessions retain their authored-report retry budget. An
    // explicit coupled slice can reserve even below eight; new failures can
    // still yield early in either mode while two shared requests remain.
    if reserve == 0 || remaining <= 2 || (remaining < 8 && limit.is_none()) {
        return remaining;
    }
    let reserved = if remaining >= 8 {
        reserve.max(pending.saturating_add(3)).min(remaining - 1)
    } else {
        2
    };
    (remaining - reserved).min(limit.unwrap_or(8)).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_slice_leaves_an_observer_and_continuation_when_possible() {
        for remaining in 3..=64 {
            let allowed = allowance(remaining, 3, 0, Some(8));
            assert!(allowed > 0 && allowed <= 8);
            assert!(remaining - allowed >= 2);
        }
        assert_eq!(allowance(6, 3, 0, Some(4)), 4);
        assert_eq!(allowance(20, 3, 0, Some(8)), 8);
        assert_eq!(allowance(20, 3, 18, Some(8)), 1);
        assert_eq!(allowance(2, 3, 0, Some(8)), 2);
        assert_eq!(allowance(3, 3, 0, None), 3);
        assert_eq!(allowance(32, 0, 0, None), 32);
    }
}
