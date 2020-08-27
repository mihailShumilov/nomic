use orga::encoding::{self as ed, Decode, Encode};
use crate::core::primitives::Number;

#[derive(Debug, Default, PartialEq, Encode, Decode)]
pub struct Account {
    pub nonce: u64,
    pub balance: u64,
}

pub struct Position {
    pub size: u64,
    pub entry_price: Number,
}