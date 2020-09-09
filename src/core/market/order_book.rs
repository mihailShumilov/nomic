use crate::core::primitives::Address;
use crate::Result;
use orga::{
    collections::Entry,
    collections::Map,
    encoding::{self as ed, Decode, Encode},
    macros::state,
    state::State,
    Store,
};
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
// TODO: move this somewhere else
pub const SATOSHIS_PER_BITCOIN: u64 = 100_000_000;
impl Order {
    pub fn cost(&self) -> u64 {
        // price is cents per bitcoin
        // size is cents
        // cost is satoshis
        self.size * SATOSHIS_PER_BITCOIN / self.price
    }
}

#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub struct Bid(pub Order);

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
pub struct Ask(pub Order);

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

#[state]
pub struct OrderBookState<S: Store> {
    pub bids: EntryMap<Bid>,
    pub asks: EntryMap<Ask>,
}

pub struct EntryMap<S: Store, T: Entry> {
    map: Map<S, T::Key, T::Value>,
}

impl<S: Store, T: Entry> State<S> for EntryMap<S, T> {
    fn wrap_store(store: S) -> orga::Result<Self> {
        Ok(Self { map: store.wrap()? })
    }
}

impl<S: Store, T: Entry> EntryMap<S, T> {
    pub fn insert(&mut self, entry: T) -> Result<()> {
        let (key, value) = entry.into_entry();
        self.map.insert(key, value)
    }

    pub fn delete(&mut self, entry: T) -> Result<()> {
        let (key, _value) = entry.into_entry();
        self.map.delete(key)
    }
}

impl<S: Store + orga::store::Iter, T: Entry> EntryMap<S, T> {
    pub fn iter(&self) -> EntryMapIter<'_, T, S> {
        let backing_iter = self.map.iter();
        EntryMapIter {
            backing_iter,
            phantom_a: std::marker::PhantomData,
        }
    }
}

pub struct EntryMapIter<'a, T, S>
where
    T: Entry,
    S: orga::store::Read + orga::store::Iter,
{
    backing_iter: orga::collections::map::Iter<'a, S::Iter<'a>, T::Key, T::Value>,
    phantom_a: std::marker::PhantomData<&'a ()>,
}
impl<T, S> Iterator for EntryMapIter<'_, T, S>
where
    T: Entry,
    S: orga::store::Read + orga::store::Iter,
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
#[derive(Copy, Clone)]
pub enum Side {
    Buy,
    Sell,
}
pub enum OrderOptions {
    Limit { side: Side, price: u64, size: u64 },
    Market { side: Side, size: u64 },
}

impl<S> OrderBookState<S>
where
    S: Store + orga::store::Iter,
{
    pub fn place_limit_sell(
        &mut self,
        size: u64,
        creator: &Address,
        price: u64,
        height: u64,
    ) -> Result<PlaceResult> {
        let match_result = match_orders(&mut self.bids, size, creator, Side::Sell, Some(price))?;

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
        let match_result = match_orders(&mut self.asks, size, creator, Side::Buy, Some(price))?;

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
        match_orders(&mut self.asks, size, creator, Side::Buy, None)
    }

    pub fn place_market_sell(&mut self, size: u64, creator: &Address) -> Result<PlaceResult> {
        match_orders(&mut self.bids, size, creator, Side::Sell, None)
    }

    pub fn cancel_order(&mut self, side: Side, key: OrderKey) -> Result<()> {
        let order = Order {
            size: 0,
            creator: key.creator,
            height: key.height,
            price: key.price,
        };
        match side {
            Side::Sell => self.asks.delete(Ask(order)),
            Side::Buy => self.bids.delete(Bid(order)),
        }
    }
}

fn match_orders<S, T>(
    orders: &mut EntryMap<S, T>,
    size: u64,
    creator: &Address,
    side: Side,
    price: Option<u64>,
) -> Result<PlaceResult>
where
    S: Store + orga::store::Iter,
    T: DerefMut<Target = Order> + Entry + Copy,
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

#[cfg(test)]
mod tests {
    use super::*;
    use orga::store::{MapStore, Read};

    #[test]
    fn ordering() {
        let mut state: OrderBookState<_> = MapStore::new().wrap().unwrap();

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
        let mut state: OrderBookState<_> = MapStore::new().wrap().unwrap();
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
        let mut state: OrderBookState<_> = MapStore::new().wrap().unwrap();
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

    #[test]
    fn order_cancellation() {
        let mut state: OrderBookState<_> = MapStore::new().wrap().unwrap();
        state.place_limit_sell(10, &[2; 33], 10, 42).unwrap();
        state.place_limit_sell(10, &[2; 33], 9, 42).unwrap();
        state
            .cancel_order(
                Side::Sell,
                OrderKey {
                    price: 10,
                    height: 42,
                    creator: [2; 33],
                },
            )
            .unwrap();
        let res = state.place_market_buy(20, &[3; 33]).unwrap();
        assert_eq!(
            res,
            PlaceResult {
                total_fill_size: 10,
                fills: vec![Order {
                    price: 9,
                    height: 42,
                    size: 10,
                    creator: [2; 33],
                }],
            },
        );
    }
}
