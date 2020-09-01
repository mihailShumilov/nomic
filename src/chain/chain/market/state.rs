use crate::core::market::OrderBookState;
use orga::{macros::state, store::Iter, Store};

#[state]
pub struct State<S: Store> {
    pub orders: OrderBookState,
}
