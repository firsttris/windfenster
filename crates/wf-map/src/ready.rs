//! Which hours of a run are ready to fetch. The DWD writes a run's files one variable at a time
//! (U, V and the gust of one hour minutes apart), so an hour is taken once U and V are listed
//! and its gust is too; without the gust only when the model has moved past the hour (a later
//! gust is listed), or at step 0, which has no gust. Otherwise an hour fetched between two files
//! would stay without gusts for the rest of the run.

/// An hour to fetch, and whether its gust file is out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ready {
    pub step: u32,
    pub gust: bool,
}

/// The steps listed for U and V, in order, with their gusts where listed; a step whose gust is
/// not out yet waits, unless a later gust is listed or it is step 0.
pub fn ready_steps(u: &[u32], v: &[u32], gusts: &[u32]) -> Vec<Ready> {
    let latest_gust = gusts.iter().copied().max();
    let mut steps: Vec<u32> = u.iter().copied().filter(|s| v.contains(s)).collect();
    steps.sort_unstable();
    steps.dedup();
    steps
        .into_iter()
        .filter_map(|step| {
            if gusts.contains(&step) {
                Some(Ready { step, gust: true })
            } else if step == 0 || latest_gust.is_some_and(|g| g > step) {
                Some(Ready { step, gust: false })
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(step: u32, gust: bool) -> Ready {
        Ready { step, gust }
    }

    #[test]
    fn takes_hours_with_their_gust_and_waits_for_a_gust_still_to_come() {
        // U and V of hours 0 to 3 are out, the gusts of 1 and 2: hour 3 waits for its gust
        assert_eq!(
            ready_steps(&[0, 1, 2, 3], &[3, 2, 1, 0], &[1, 2]),
            vec![r(0, false), r(1, true), r(2, true)]
        );
        // Hour 1 has no gust but hour 2 has: the DWD is past hour 1, it is drawn without
        assert_eq!(
            ready_steps(&[0, 1, 2], &[0, 1, 2], &[2]),
            vec![r(0, false), r(1, false), r(2, true)]
        );
        // No gust out at all: only step 0, which never has one
        assert_eq!(ready_steps(&[0, 1], &[0, 1], &[]), vec![r(0, false)]);
        // V not out yet: nothing
        assert_eq!(ready_steps(&[0, 1], &[], &[0, 1]), vec![]);
    }
}
