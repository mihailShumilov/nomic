use super::{accounts, market, peg, work, Action};
use crate::core::primitives::transaction::Transaction;
use crate::core::primitives::Result;
use orga::{macros::state, Store};
use std::collections::BTreeMap;

#[state]
pub struct State<S: Store> {
    pub peg: peg::State,
    pub accounts: accounts::State,
    pub work: work::State,
    pub market: market::State,
}

pub fn run<S: Store>(
    store: S,
    action: Action,
    validators: &mut BTreeMap<Vec<u8>, u64>,
    height: u64,
) -> Result<()> {
    let mut state: State<_> = store.wrap()?;

    #[cfg_attr(rustfmt, rustfmt_skip)]
    match action {
        Action::Transaction(tx) => match tx {
            // Peg transactions
            Transaction::Deposit(tx) =>
                peg::handlers::deposit_tx(&mut state.peg, &mut state.accounts, tx),
            Transaction::Withdrawal(tx) =>
                peg::handlers::withdrawal_tx(&mut state.peg, &mut state.accounts, tx),
            Transaction::Signature(tx) =>
                peg::handlers::signature_tx(&mut state.peg, tx),
            Transaction::Header(tx) =>
                peg::handlers::header_tx(&mut state.peg, tx),

            // Account transactions
            Transaction::Transfer(tx) =>
                accounts::handlers::transfer_tx(&mut state.accounts, tx),

            // Validator transactions
            Transaction::WorkProof(tx) =>
                work::handlers::work_proof_tx(&mut state.work, validators, tx),

            // Market transactions
            Transaction::PlaceOrder(tx) => market::handlers::place_order_tx(&mut state.market, &mut state.accounts, tx, height),
            // Transaction::UpdateOrder(tx) => market::handlers::update_order_tx(&mut state.market, tx),
            _ => failure::bail!("remove this line"),
        },
        Action::BeginBlock(header) => {
            peg::handlers::begin_block(&mut state.peg, validators, header)
        }
    }
}

// TODO: this should be Action::InitChain
/// Called once at genesis to write some data to the store.
pub fn initialize<S: Store>(store: S) -> Result<()> {
    let mut state: State<_> = store.wrap()?;
    peg::handlers::initialize(&mut state.peg)
}
