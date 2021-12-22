use crate::bitcoin::header_queue::HeaderList;
use crate::bitcoin::header_queue::HeaderQueue;
use crate::bitcoin::relayer::DepositTxn;
use crate::bitcoin::relayer::PegClient;
use crate::error::{Error, Result};
use std::sync::{Arc, Mutex};

pub struct Peg {
    headers: HeaderQueue,
}

impl Peg {
    pub fn new(headers: HeaderQueue) -> Peg {
        Peg { headers }
    }
}

impl PegClient for Arc<Mutex<Peg>> {
    fn height(&self) -> Result<u32> {
        self.lock().unwrap().headers.height()
    }

    fn trusted_height(&self) -> Result<u32> {
        Ok(self.lock().unwrap().headers.trusted_height())
    }

    fn add(&mut self, header: HeaderList) -> Result<()> {
        Ok(self.lock().unwrap().headers.add(header)?)
    }

    fn verify_deposit(&self, deposit: DepositTxn) -> Result<bool> {
        let header = match self
            .lock()
            .unwrap()
            .headers
            .get_by_height(deposit.block_height)?
        {
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

        if header_merkle_root != proof_merkle_root {
            Ok(false)
        } else {
            Ok(true)
        }
    }
}
