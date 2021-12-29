use crate::bitcoin::checkpoint_set::CheckpointSet;
use crate::bitcoin::header_queue::Config;
use crate::bitcoin::header_queue::HeaderList;
use crate::bitcoin::header_queue::HeaderQueue;
use crate::bitcoin::relayer::DepositTxn;
use crate::error::{Error, Result};
use orga::call::Call;
use orga::client::Client;
use orga::query::Query;
use orga::state::State;
use orga::store::Store;
use orga::Result as OrgaResult;

#[derive(State, Call, Query, Client)]
pub struct Peg {
    headers: HeaderQueue,
}

impl Peg {
    #[query]
    pub fn trusted_height(&self) -> OrgaResult<u32> {
        Ok(self.headers.trusted_height())
    }

    #[query]
    pub fn height(&self) -> OrgaResult<u32> {
        self.headers.height()
    }

    #[call]
    pub fn add(&mut self, header: HeaderList) -> OrgaResult<()> {
        Ok(self.headers.add(header)?)
    }

    fn get_signatory_set(&self) -> Result<CheckpointSet> {
        unimplemented!()
    }

    fn verify_deposit(&self, deposit: DepositTxn) -> Result<bool> {
        let header = match self.headers.get_by_height(deposit.block_height)? {
            Some(header) => header,
            None => {
                return Err(Error::Relayer(format!(
                    "No header exists at height {}",
                    deposit.block_height
                )))
            }
        };

        let header_merkle_root = header.merkle_root;
        let proof_merkle_root = match deposit.proof.extract_matches(
            &mut vec![deposit.transaction.txid()],
            &mut vec![deposit.index],
        ) {
            Ok(merkle_root) => merkle_root,
            Err(_) => {
                return Err(Error::Relayer(
                    "Failed to extract merkle root from proof".to_string(),
                ))
            }
        };

        Ok(header_merkle_root == proof_merkle_root)
    }
}
