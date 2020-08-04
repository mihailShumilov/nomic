use crate::core::primitives::Address;
use crate::Result;
use orga::{
    collections::{Map, Set},
    state, Decode, Encode, Entry, Iter, MapStore, Store, Value,
};

use std::marker::PhantomData;

#[derive(Encode, Decode, Debug)]
struct OrderKey {
    pub price: u64,
    pub creator: Address,
    pub height: u64,
}
#[derive(Encode, Decode, Debug)]
struct OrderValue {
    pub size: u64,
}
#[derive(Eq, PartialEq, Debug, Clone, Copy)]
struct Order {
    pub price: u64,
    pub creator: Address,
    pub height: u64,
    pub size: u64,
}

#[derive(Eq, PartialEq, Debug, Clone, Copy)]
struct Bid(Order);

impl Entry for Bid {
    type Key = OrderKey;
    type Value = OrderValue;

    fn into_entry(self) -> (OrderKey, OrderValue) {
        let Order {
            price,
            creator,
            height,
            size,
        } = self.0;
        (
            OrderKey {
                price: u64::MAX - price,
                creator,
                height,
            },
            OrderValue { size },
        )
    }

    fn from_entry(entry: (Self::Key, Self::Value)) -> Self {
        Self(Order {
            price: u64::MAX - entry.0.price,
            size: entry.1.size,
            height: entry.0.height,
            creator: entry.0.creator,
        })
    }
}

struct Ask(Order);

impl Entry for Ask {
    type Key = OrderKey;
    type Value = OrderValue;

    fn into_entry(self) -> (OrderKey, OrderValue) {
        let Order {
            price,
            creator,
            height,
            size,
        } = self.0;
        (
            OrderKey {
                price,
                creator,
                height,
            },
            OrderValue { size },
        )
    }

    fn from_entry(entry: (Self::Key, Self::Value)) -> Self {
        Self(Order {
            price: entry.0.price,
            size: entry.1.size,
            height: entry.0.height,
            creator: entry.0.creator,
        })
    }
}

pub struct OrderBookState {
    bids: EntryMap<Bid>,
    asks: EntryMap<Ask>,
}

struct EntryMap<T: Entry> {
    map: Map<MapStore, T::Key, T::Value>,
}

impl<T: Entry> EntryMap<T> {
    pub fn insert(&mut self, entry: T) -> Result<()> {
        let (key, value) = entry.into_entry();
        self.map.insert(key, value)
    }
    pub fn new() -> Self {
        Self {
            map: MapStore::new().wrap().unwrap(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = T> + '_ {
        let backing_iter = self.map.iter();
        EntryMapIter {
            backing_iter,
            phantom_T: PhantomData,
        }
    }
}

struct EntryMapIter<T, B>
where
    T: Entry,
    B: Iterator<Item = (T::Key, T::Value)>,
{
    backing_iter: B,
    phantom_T: PhantomData<T>,
}
impl<T, B> Iterator for EntryMapIter<T, B>
where
    T: Entry,
    B: Iterator<Item = (T::Key, T::Value)>,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.backing_iter.next().map(Entry::from_entry)
    }
}

impl OrderBookState {
    pub fn new() -> Self {
        OrderBookState {
            bids: EntryMap::new(),
            asks: EntryMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    #[test]
    fn basic() {
        let mut state = OrderBookState::new();

        let bid1 = Bid(Order {
            height: 42,
            size: 10,
            price: 10000,
            creator: [2; 33],
        });
        let bid2 = Bid(Order {
            height: 42,
            size: 10,
            price: 11000,
            creator: [2; 33],
        });
        let bid3 = Bid(Order {
            height: 42,
            size: 10,
            price: 9000,
            creator: [2; 33],
        });

        state.bids.insert(bid1).unwrap();
        state.bids.insert(bid2).unwrap();
        state.bids.insert(bid3).unwrap();

        let mut bids = state.bids.iter();
        assert_eq!(bids.next().unwrap(), bid2);
        assert_eq!(bids.next().unwrap(), bid1);
        assert_eq!(bids.next().unwrap(), bid3);
    }
}
