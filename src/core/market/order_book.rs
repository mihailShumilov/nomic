use crate::core::primitives::Address;
use crate::Result;
use orga::{collections::Map, Decode, Encode, Entry, MapStore, Store};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

#[derive(Encode, Decode, Debug)]
pub struct OrderKey {
    pub price: u64,
    pub creator: Address,
    pub height: u64,
}
#[derive(Encode, Decode, Debug)]
pub struct OrderValue {
    pub size: u64,
}
#[derive(Eq, PartialEq, Debug, Encode, Decode, Clone, Copy)]
pub struct Order {
    pub price: u64,
    pub creator: Address,
    pub height: u64,
    pub size: u64,
}

#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub struct Bid(Order);

impl Deref for Bid {
    type Target = Order;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Bid {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

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

#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub struct Ask(Order);

impl Deref for Ask {
    type Target = Order;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Ask {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

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

    pub fn delete(&mut self, entry: T) -> Result<()> {
        let (key, _value) = entry.into_entry();
        self.map.delete(key)
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
            phantom_t: PhantomData,
        }
    }
}

struct EntryMapIter<T, B>
where
    T: Entry,
    B: Iterator<Item = (T::Key, T::Value)>,
{
    backing_iter: B,
    phantom_t: PhantomData<T>,
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

#[derive(Debug, Encode, Decode, Default, PartialEq, Eq)]
pub struct PlaceResult {
    pub total_fill_size: u64,
    pub fills: Vec<Order>,
}
pub enum Side {
    Buy,
    Sell,
}
pub enum OrderOptions {
    Limit { side: Side, price: u64, size: u64 },
    Market { side: Side, size: u64 },
}

impl OrderBookState {
    pub fn new() -> Self {
        OrderBookState {
            bids: EntryMap::new(),
            asks: EntryMap::new(),
        }
    }

    fn match_orders<I>(&self, order_iter: I, size: u64) -> Result<PlaceResult>
    where
        I: Iterator<Item = Order>,
    {
        let mut result = PlaceResult::default();

        loop {
            let remaining_size = size - result.total_fill_size;
            let next_ask = self.asks.iter().next();
            let mut next_ask = match next_ask {
                Some(a) => a,
                None => break,
            };

            if remaining_size >= next_ask.0.size {
                // Completely filling the ask, removing it from order book.
                result.total_fill_size += next_ask.0.size;
                result.fills.push(next_ask.0.clone());
                self.asks.delete(next_ask)?;
            } else {
                // Partially filling the ask, updating it on the order book.
                result.total_fill_size += remaining_size;
                let mut partial_fill = next_ask.0.clone();
                partial_fill.size = remaining_size;
                result.fills.push(partial_fill);
            }

            if size == result.total_fill_size {
                break;
            }
        }

        Ok(result)
    }

    pub fn place_market_buy(&mut self, size: u64, creator: &Address) -> Result<PlaceResult> {
        let order_iter = self.asks.iter().map(|a| a.0);
        self.match_orders(order_iter, size, creator)
    }

    pub fn place_limit_sell(
        &mut self,
        price: u64,
        size: u64,
        creator: &Address,
    ) -> Result<PlaceResult> {
        let result = PlaceResult::default();

        // Match against existing orders before possibly writing to the book.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering() {
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

        let ask1 = Ask(Order {
            height: 42,
            size: 10,
            price: 10000,
            creator: [2; 33],
        });
        let ask2 = Ask(Order {
            height: 42,
            size: 10,
            price: 11000,
            creator: [2; 33],
        });
        let ask3 = Ask(Order {
            height: 42,
            size: 10,
            price: 9000,
            creator: [2; 33],
        });
        state.asks.insert(ask1).unwrap();
        state.asks.insert(ask2).unwrap();
        state.asks.insert(ask3).unwrap();

        let mut bids = state.bids.iter();
        assert_eq!(bids.next().unwrap(), bid2);
        assert_eq!(bids.next().unwrap(), bid1);
        assert_eq!(bids.next().unwrap(), bid3);

        let mut asks = state.asks.iter();
        assert_eq!(asks.next().unwrap(), ask3);
        assert_eq!(asks.next().unwrap(), ask1);
        assert_eq!(asks.next().unwrap(), ask2);
    }

    #[test]
    fn matching() {
        let mut state = OrderBookState::new();
        state.asks.insert(Ask(Order {
            creator: [2; 33],
            height: 42,
            size: 10,
            price: 10,
        }));
        state.asks.insert(Ask(Order {
            creator: [2; 33],
            height: 42,
            size: 10,
            price: 30,
        }));

        let res = state.place_market_buy(15, &[2; 33]).unwrap();
        println!("{:?}", res);
        assert_eq!(
            res,
            PlaceResult {
                total_fill_size: 15,
                fills: vec![
                    Order {
                        price: 10,
                        creator: [2; 33],
                        height: 42,
                        size: 10
                    },
                    Order {
                        price: 30,
                        creator: [2; 33],
                        height: 42,
                        size: 5
                    }
                ],
            }
        )
    }
}
