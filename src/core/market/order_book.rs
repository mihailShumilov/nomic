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

    fn match_orders<T>(
        orders: &mut EntryMap<T>,
        size: u64,
        creator: &Address,
        side: &Side,
        price: Option<u64>,
    ) -> Result<PlaceResult>
    where
        T: Deref<Target = Order> + DerefMut + Entry + Copy,
    {
        // TODO: order cost limiting
        let mut result = PlaceResult::default();
        let mut orders_to_delete = vec![];
        let mut order_to_insert = None;
        for mut next_order in orders.iter() {
            let remaining_size = size - result.total_fill_size;

            match (side, price) {
                (Side::Buy, Some(price)) if price < next_order.price => break,
                (Side::Sell, Some(price)) if price > next_order.price => break,
                _ => (),
            };
            if &next_order.creator == creator {
                continue;
            }
            if remaining_size >= next_order.size {
                // Completely filling the ask, removing it from order book.
                result.total_fill_size += next_order.size;
                result.fills.push(*next_order);
                // orders.delete(next_order)?;
                orders_to_delete.push(next_order);
            } else {
                // Partially filling the ask, updating it on the order book.
                result.total_fill_size += remaining_size;
                let mut partial_fill = next_order;
                partial_fill.size = remaining_size;
                result.fills.push(*partial_fill);
                next_order.size -= remaining_size;
                order_to_insert = Some(next_order);
            }

            if size == result.total_fill_size {
                break;
            }
        }

        for order in orders_to_delete {
            orders.delete(order)?;
        }
        if let Some(order) = order_to_insert {
            orders.insert(order)?;
        }

        Ok(result)
    }

    pub fn place_limit_sell(
        &mut self,
        size: u64,
        creator: &Address,
        price: u64,
        height: u64,
    ) -> Result<PlaceResult> {
        let match_result =
            Self::match_orders(&mut self.bids, size, creator, &Side::Sell, Some(price))?;

        // Place unfilled part of order into order book.
        self.asks.insert(Ask(Order {
            price,
            creator: *creator,
            height,
            size: size - match_result.total_fill_size,
        }))?;

        Ok(match_result)
    }

    pub fn place_limit_buy(
        &mut self,
        size: u64,
        creator: &Address,
        price: u64,
        height: u64,
    ) -> Result<PlaceResult> {
        let match_result =
            Self::match_orders(&mut self.asks, size, creator, &Side::Buy, Some(price))?;

        // Place unfilled part of order into order book.
        self.bids.insert(Bid(Order {
            price,
            creator: *creator,
            height,
            size: size - match_result.total_fill_size,
        }))?;

        Ok(match_result)
    }

    pub fn place_market_buy(&mut self, size: u64, creator: &Address) -> Result<PlaceResult> {
        Self::match_orders(&mut self.asks, size, creator, &Side::Buy, None)
    }

    pub fn place_market_sell(&mut self, size: u64, creator: &Address) -> Result<PlaceResult> {
        Self::match_orders(&mut self.bids, size, creator, &Side::Sell, None)
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
    fn partial_matching() {
        let mut state = OrderBookState::new();
        state
            .asks
            .insert(Ask(Order {
                creator: [2; 33],
                height: 42,
                size: 10,
                price: 10,
            }))
            .unwrap();
        state
            .asks
            .insert(Ask(Order {
                creator: [3; 33],
                height: 42,
                size: 10,
                price: 30,
            }))
            .unwrap();

        let res = state.place_market_buy(15, &[4; 33]).unwrap();
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
                        creator: [3; 33],
                        height: 42,
                        size: 5
                    }
                ],
            }
        )
    }

    #[test]
    fn order_placement_methods() {
        let mut state = OrderBookState::new();
        state.place_limit_sell(20, &[2; 33], 10, 42).unwrap();
        state.place_limit_sell(20, &[2; 33], 12, 42).unwrap();
        let res = state.place_limit_buy(25, &[3; 33], 11, 42).unwrap();
        assert_eq!(
            res,
            PlaceResult {
                total_fill_size: 20,
                fills: vec![Order {
                    price: 10,
                    creator: [2; 33],
                    size: 20,
                    height: 42,
                }]
            }
        );
        let res = state.place_market_buy(5, &[3; 33]).unwrap();
        assert_eq!(
            res,
            PlaceResult {
                total_fill_size: 5,
                fills: vec![Order {
                    price: 12,
                    creator: [2; 33],
                    size: 5,
                    height: 42,
                }]
            }
        );
    }
}
