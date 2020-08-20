use crate::core::market::OrderBookState;
use orga::{macros::state, Store};

#[state]
pub struct State<S: Store> {
    orders: OrderBookState,
}
