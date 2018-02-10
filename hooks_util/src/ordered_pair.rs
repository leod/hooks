#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub struct OrderedPair<T>((T, T));

impl<T: Ord> OrderedPair<T> {
    pub fn new(first: T, second: T) -> OrderedPair<T> {
        if first <= second {
            OrderedPair((first, second))
        } else {
            OrderedPair((second, first))
        }
    }
}
