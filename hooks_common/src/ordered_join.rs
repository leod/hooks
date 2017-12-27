use std::iter::Peekable;

/// Element of a full join
pub enum Item<K, T, U> {
    /// The key `K` is only contained in the left iterator
    Left(K, T),

    /// The key `K` is only contained in the right iterator
    Right(K, U),

    /// The key `K` is contained in both iterators
    Both(K, T, U),
}

impl<K, T, U> Item<K, T, U> {
    // Which iterators need to be advanced?
    pub fn next_flags(&self) -> (bool, bool) {
        match self {
            &Item::Left(_, _) => (true, false),
            &Item::Right(_, _) => (false, true),
            &Item::Both(_, _, _) => (true, true)
        }
    }
}

enum ItemKind {
    Both,
    Left,
    Right,
}

pub fn full_join_item<K, T, U>(
    left: Option<(K, T)>,
    right: Option<(K, U)>,
) -> Option<Item<K, T, U>>
where
    K: Ord
{
    match (left, right) {
        (Some((left_k, t)), Some((right_k, u))) => {
            if left_k < right_k {
                Some(Item::Left(left_k, t))
            } else if right_k < left_k {
                Some(Item::Right(right_k, u))
            } else {
                assert!(left_k == right_k);
                Some(Item::Both(left_k, t, u))
            }
        }
        (Some((left_k, t)), None) =>
            Some(Item::Left(left_k, t)),
        (None, Some((right_k, u))) =>
            Some(Item::Right(right_k, u)),
        (None, None) =>
            None,
    }
}

/// Iterator over the full join of two sequences of key value pairs. The
/// sequences are assumed to be sorted by the key in ascending order.
pub struct FullJoinIter<LeftIter, RightIter, K, T, U>
where
    LeftIter: Iterator<Item = (K, T)>,
    RightIter: Iterator<Item = (K, U)>,
    K: Ord,
{
    left_iter: Peekable<LeftIter>,
    right_iter: Peekable<RightIter>,
}

impl<LeftIter, RightIter, K, T, U> FullJoinIter<LeftIter, RightIter, K, T, U>
where
    LeftIter: Iterator<Item = (K, T)>,
    RightIter: Iterator<Item = (K, U)>,
    K: Ord,
{
    pub fn new(left_iter: LeftIter, right_iter: RightIter) -> Self {
        Self {
            left_iter: left_iter.peekable(),
            right_iter: right_iter.peekable(),
        }
    }
}

impl<LeftIter, RightIter, K, T, U> Iterator for FullJoinIter<LeftIter, RightIter, K, T, U>
where
    LeftIter: Iterator<Item = (K, T)>,
    RightIter: Iterator<Item = (K, U)>,
    K: Ord,
{
    type Item = Item<K, T, U>;

    fn next(&mut self) -> Option<Self::Item> {
        // Which iterator has the element with the smaller key?
        let kind = match (self.left_iter.peek(), self.right_iter.peek()) {
            (Some(&(ref left_k, _)), Some(&(ref right_k, _))) => if left_k < right_k {
                Some(ItemKind::Left)
            } else if right_k < left_k {
                Some(ItemKind::Right)
            } else {
                Some(ItemKind::Both)
            },
            (Some(_), None) => Some(ItemKind::Left),
            (None, Some(_)) => Some(ItemKind::Right),
            (None, None) => None,
        };

        // Advance iterators with the smaller key
        match kind {
            Some(ItemKind::Both) => {
                let left = self.left_iter.next().unwrap();
                let right = self.right_iter.next().unwrap();
                assert!(left.0 == right.0);

                Some(Item::Both(left.0, left.1, right.1))
            }
            Some(ItemKind::Left) => {
                let left = self.left_iter.next().unwrap();
                Some(Item::Left(left.0, left.1))
            }
            Some(ItemKind::Right) => {
                let right = self.right_iter.next().unwrap();
                Some(Item::Right(right.0, right.1))
            }
            None => None,
        }
    }
}
