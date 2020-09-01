use super::super::accounts::State as AccountState;
use super::super::SECP;
use super::State as MarketState;
use crate::core::market::Direction;
use crate::core::primitives::{transaction::PlaceOrderTransaction, Account, Address};
use crate::Result;
use failure::bail;
use orga::{
    store::{Iter, MapStore, Read, Write},
    Store,
};

pub fn place_order_tx<S: Store + Iter>(
    market: &mut MarketState<S>,
    accounts: &mut AccountState<S>,
    height: u64,
    tx: PlaceOrderTransaction,
) -> Result<()> {
    if tx.fee_amount < 1000 {
        bail!("Transaction fee is too small");
    }
    // TODO: Update this when tx encoding uses Ed instead of Serde
    if tx.creator.len() != 33 {
        bail!("Invalid creator address");
    }
    let creator = unsafe_slice_to_address(&tx.creator[..]);
    if !tx.verify_signature(&SECP)? {
        bail!("Invalid signature");
    }
    let maybe_creator_account = accounts.get(unsafe_slice_to_address(&tx.creator[..]))?;
    let mut creator_account = match maybe_creator_account {
        Some(creator_account) => creator_account,
        None => bail!("Account does not exist"),
    };
    if tx.nonce != creator_account.nonce {
        bail!("Invalid account nonce");
    }
    let place_result = match (tx.side, tx.price) {
        (Direction::Long, Some(price)) => market
            .orders
            .place_limit_buy(tx.size, &creator, price, height)?,
        (Direction::Short, Some(price)) => market
            .orders
            .place_limit_sell(tx.size, &creator, price, height)?,
        (Direction::Long, None) => market.orders.place_market_buy(tx.size, &creator)?,
        (Direction::Short, None) => market.orders.place_market_sell(tx.size, &creator)?,
    };

    for fill in place_result.fills.iter() {}
    // TODO: Check that fill cost is less than account balance

    creator_account.nonce += 1;
    Ok(())
}

fn unsafe_slice_to_address(slice: &[u8]) -> Address {
    let mut buf: Address = [0; 33];
    buf.copy_from_slice(slice);
    buf
}
#[cfg(test)]
mod tests {
    use super::super::super::test_utils;
    use super::*;
    use crate::core::primitives::transaction::Transaction;

    #[test]
    fn place_order_ok() {
        let (privkey, pubkey) = test_utils::create_keypair(2);
        let mut tx = PlaceOrderTransaction {
            creator: pubkey.serialize().to_vec(),
            signature: vec![],
            nonce: 0,
            fee_amount: 1000,
            price: Some(10000),
            side: Direction::Long,
            size: 100,
        };
        tx.signature = test_utils::sign(&mut tx, privkey);
        let mut account_state: AccountState<_> = MapStore::new().wrap().unwrap();
        let mut market_state: MarketState<_> = MapStore::new().wrap().unwrap();
        account_state
            .insert(
                pubkey.serialize(),
                Account {
                    nonce: 0,
                    balance: 100000,
                },
            )
            .unwrap();
        place_order_tx(&mut market_state, &mut account_state, 42, tx).unwrap();

        // TODO: More state assertions
    }

    #[test]
    #[should_panic(expected = "Invalid signature")]
    fn place_order_invalid_signature() {
        let (privkey, pubkey) = test_utils::create_keypair(2);
        let mut tx = PlaceOrderTransaction {
            creator: pubkey.serialize().to_vec(),
            signature: vec![],
            nonce: 0,
            fee_amount: 1000,
            price: Some(10000),
            side: Direction::Long,
            size: 100,
        };
        tx.signature = test_utils::sign(&mut tx, privkey);
        tx.signature[10] ^= 1;
        let mut account_state: AccountState<_> = MapStore::new().wrap().unwrap();
        let mut market_state: MarketState<_> = MapStore::new().wrap().unwrap();
        account_state
            .insert(
                pubkey.serialize(),
                Account {
                    nonce: 0,
                    balance: 100000,
                },
            )
            .unwrap();
        place_order_tx(&mut market_state, &mut account_state, 42, tx).unwrap();
    }
    #[test]
    #[should_panic(expected = "Account does not exist")]
    fn place_order_from_nonexistent_account() {
        let (privkey, pubkey) = test_utils::create_keypair(2);
        let mut tx = PlaceOrderTransaction {
            creator: pubkey.serialize().to_vec(),
            signature: vec![],
            nonce: 0,
            fee_amount: 1000,
            price: Some(10000),
            side: Direction::Long,
            size: 100,
        };
        tx.signature = test_utils::sign(&mut tx, privkey);
        let mut account_state: AccountState<_> = MapStore::new().wrap().unwrap();
        let mut market_state: MarketState<_> = MapStore::new().wrap().unwrap();
        place_order_tx(&mut market_state, &mut account_state, 42, tx).unwrap();
    }
}
