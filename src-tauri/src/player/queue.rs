//! The play queue: track ids in list order plus a play order (identity, or a
//! shuffle that starts at the current track). Pure logic, no I/O.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

#[derive(Debug, Default)]
pub struct Queue {
    items: Vec<u64>,
    /// Indexes into `items`, in play order.
    order: Vec<usize>,
    /// Position in `order` of the current track.
    pos: usize,
    shuffle: bool,
    repeat: Repeat,
    seed: u64,
}

impl Queue {
    pub fn with_seed(seed: u64) -> Self {
        Self {
            seed: seed | 1,
            ..Self::default()
        }
    }

    /// Replace the queue with `items`, starting at `items[start]`.
    pub fn replace(&mut self, items: Vec<u64>, start: usize) {
        let start = start.min(items.len().saturating_sub(1));
        self.items = items;
        self.reorder(start);
    }

    pub fn current(&self) -> Option<u64> {
        self.order.get(self.pos).map(|&index| self.items[index])
    }

    /// Index of the current track in list order.
    pub fn index(&self) -> Option<usize> {
        self.order.get(self.pos).copied()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    pub fn repeat(&self) -> Repeat {
        self.repeat
    }

    pub fn set_repeat(&mut self, repeat: Repeat) {
        self.repeat = repeat;
    }

    /// Turning shuffle on keeps the current track and shuffles the rest after
    /// it; turning it off continues in list order from the current track.
    pub fn set_shuffle(&mut self, shuffle: bool) {
        if self.shuffle == shuffle {
            return;
        }
        self.shuffle = shuffle;
        let current = self.index().unwrap_or(0);
        self.reorder(current);
    }

    /// The track that plays when the current one ends, without moving.
    pub fn peek_after_end(&self) -> Option<u64> {
        match self.repeat {
            Repeat::One => self.current(),
            _ => self.step_forward().map(|pos| self.items[self.order[pos]]),
        }
    }

    /// The current track ended: repeat it, move on, or stop at the end.
    pub fn advance_after_end(&mut self) -> Option<u64> {
        if self.repeat == Repeat::One {
            return self.current();
        }
        self.skip_forward()
    }

    /// The user pressed "next": move on even in repeat-one; at the end of
    /// the queue wrap only with repeat-all.
    pub fn skip_forward(&mut self) -> Option<u64> {
        let pos = self.step_forward()?;
        self.pos = pos;
        self.current()
    }

    /// The user pressed "previous". At the start, wrap only with repeat-all;
    /// otherwise stay on the first track.
    pub fn skip_back(&mut self) -> Option<u64> {
        if self.order.is_empty() {
            return None;
        }
        if self.pos > 0 {
            self.pos -= 1;
        } else if self.repeat == Repeat::All {
            self.pos = self.order.len() - 1;
        }
        self.current()
    }

    /// Insert `ids` to play right after the current track.
    pub fn play_next(&mut self, ids: &[u64]) {
        self.insert(ids, self.pos + 1);
    }

    /// Append `ids` to the end of the play order.
    pub fn append(&mut self, ids: &[u64]) {
        self.insert(ids, self.order.len());
    }

    fn insert(&mut self, ids: &[u64], at: usize) {
        let first = self.items.len();
        self.items.extend_from_slice(ids);
        let at = at.min(self.order.len());
        self.order.splice(at..at, first..self.items.len());
    }

    fn step_forward(&self) -> Option<usize> {
        if self.pos + 1 < self.order.len() {
            Some(self.pos + 1)
        } else if self.repeat == Repeat::All && !self.order.is_empty() {
            Some(0)
        } else {
            None
        }
    }

    /// Rebuild the play order so that `items[start]` is current.
    fn reorder(&mut self, start: usize) {
        if self.items.is_empty() {
            self.order.clear();
            self.pos = 0;
            return;
        }
        if self.shuffle {
            let mut rest: Vec<usize> = (0..self.items.len()).filter(|&i| i != start).collect();
            self.shuffle_in_place(&mut rest);
            self.order = std::iter::once(start).chain(rest).collect();
            self.pos = 0;
        } else {
            self.order = (0..self.items.len()).collect();
            self.pos = start;
        }
    }

    /// Fisher–Yates with xorshift64: the shuffle needs no crypto quality.
    fn shuffle_in_place(&mut self, values: &mut [usize]) {
        for i in (1..values.len()).rev() {
            self.seed ^= self.seed << 13;
            self.seed ^= self.seed >> 7;
            self.seed ^= self.seed << 17;
            let j = (self.seed % (i as u64 + 1)) as usize;
            values.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue(items: &[u64], start: usize) -> Queue {
        let mut queue = Queue::with_seed(42);
        queue.replace(items.to_vec(), start);
        queue
    }

    #[test]
    fn plays_in_order_and_stops_at_the_end() {
        let mut q = queue(&[1, 2, 3], 1);
        assert_eq!(q.current(), Some(2));
        assert_eq!(q.peek_after_end(), Some(3));
        assert_eq!(q.advance_after_end(), Some(3));
        assert_eq!(q.peek_after_end(), None);
        assert_eq!(q.advance_after_end(), None);
        assert_eq!(q.current(), Some(3), "the last track stays current");
    }

    #[test]
    fn repeat_all_wraps_both_ways() {
        let mut q = queue(&[1, 2, 3], 2);
        q.set_repeat(Repeat::All);
        assert_eq!(q.advance_after_end(), Some(1));
        assert_eq!(q.skip_back(), Some(3));
    }

    #[test]
    fn repeat_one_repeats_on_end_but_next_moves_on() {
        let mut q = queue(&[1, 2], 0);
        q.set_repeat(Repeat::One);
        assert_eq!(q.peek_after_end(), Some(1));
        assert_eq!(q.advance_after_end(), Some(1));
        assert_eq!(q.skip_forward(), Some(2));
        assert_eq!(q.skip_forward(), None, "no wrap without repeat-all");
    }

    #[test]
    fn back_at_the_start_stays_on_the_first_track() {
        let mut q = queue(&[1, 2], 0);
        assert_eq!(q.skip_back(), Some(1));
    }

    #[test]
    fn shuffle_keeps_the_current_track_and_visits_every_track_once() {
        let mut q = queue(&[10, 20, 30, 40, 50, 60], 3);
        q.set_shuffle(true);
        assert_eq!(q.current(), Some(40));
        let mut seen = vec![40];
        while let Some(id) = q.skip_forward() {
            seen.push(id);
        }
        seen.sort_unstable();
        assert_eq!(seen, [10, 20, 30, 40, 50, 60]);
    }

    #[test]
    fn unshuffle_continues_in_list_order_from_the_current_track() {
        let mut q = queue(&[1, 2, 3, 4, 5], 0);
        q.set_shuffle(true);
        q.skip_forward();
        let current = q.current().unwrap();
        q.set_shuffle(false);
        assert_eq!(q.current(), Some(current));
        let expected_next = if current == 5 {
            None
        } else {
            Some(current + 1)
        };
        assert_eq!(q.peek_after_end(), expected_next);
    }

    #[test]
    fn play_next_and_append() {
        let mut q = queue(&[1, 2, 3], 0);
        q.play_next(&[9]);
        q.append(&[7]);
        let mut order = vec![q.current().unwrap()];
        while let Some(id) = q.skip_forward() {
            order.push(id);
        }
        assert_eq!(order, [1, 9, 2, 3, 7]);
        assert_eq!(q.len(), 5);
    }

    #[test]
    fn empty_queue_is_inert() {
        let mut q = queue(&[], 0);
        assert_eq!(q.current(), None);
        assert_eq!(q.skip_forward(), None);
        assert_eq!(q.skip_back(), None);
        assert_eq!(q.advance_after_end(), None);
        q.append(&[5]);
        assert_eq!(q.current(), Some(5));
    }

    #[test]
    fn start_past_the_end_clamps() {
        assert_eq!(queue(&[1, 2], 9).current(), Some(2));
    }
}
