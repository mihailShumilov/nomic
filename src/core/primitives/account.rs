use orga::encoding::{self as ed, Decode, Encode};

#[derive(Debug, Default, PartialEq, Encode, Decode)]
pub struct Account {
    pub nonce: u64,
    pub balance: u64,
}
