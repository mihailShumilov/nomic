use crate::app::InnerApp;
use crate::bitcoin::adapter::Adapter;
use crate::bitcoin::header_queue::WrappedHeader;
use crate::bitcoin::peg::Peg;
use crate::error::Result;
use bitcoin::util::merkleblock::{MerkleBlock, PartialMerkleTree};
use bitcoin::{Script, Transaction};
use bitcoincore_rpc::bitcoincore_rpc_json::ScanTxOutRequest;
use bitcoincore_rpc::{Client as BtcClient, RpcApi};
use orga::coins::Address;
use orga::encoding::{Decode, Encode};
use orga::prelude::*;
use orga::Result as OrgaResult;
use std::collections::HashMap;

const SEEK_BATCH_SIZE: u32 = 10;

#[derive(Encode, Decode)]
pub struct DepositTxn {
    pub block_height: u32,
    pub transaction: Adapter<Transaction>,
    pub index: u32,
    pub proof: Adapter<PartialMerkleTree>,
    pub payable_addr: Address,
}

type AppClient = TendermintClient<crate::app::App>;

pub struct Relayer {
    btc_client: BtcClient,
    app_client: AppClient,
    listen_map: HashMap<Script, Address>,
}

type AppQuery = <InnerApp as Query>::Query;
type PegQuery = <Peg as Query>::Query;

type AppCall = <InnerApp as Call>::Call;

impl Relayer {
    pub fn new(btc_client: BtcClient, app_client: AppClient) -> Self {
        Relayer {
            btc_client,
            app_client,
            listen_map: HashMap::new(),
        }
    }

    pub async fn app_height(&self) -> OrgaResult<u32> {
        let app_height_query = AppQuery::FieldPeg(PegQuery::MethodHeight(vec![]));
        self.app_client
            .query(app_height_query, |state| state.peg.height())
            .await
    }

    pub async fn app_trusted_height(&self) -> OrgaResult<u32> {
        let app_height_query = AppQuery::FieldPeg(PegQuery::MethodTrustedHeight(vec![]));
        self.app_client
            .query(app_height_query, |state| state.peg.trusted_height())
            .await
    }

    pub async fn start(&mut self) -> Result<!> {
        self.wait_for_trusted_header().await?;
        loop {
            self.step_header().await?;
            self.step_transaction()?;
        }
    }

    async fn wait_for_trusted_header(&self) -> Result<()> {
        loop {
            let tip_hash = self.btc_client.get_best_block_hash()?;
            let tip_height = self.btc_client.get_block_header_info(&tip_hash)?.height;
            println!("wait_for_trusted_header: btc={}", tip_height);
            let trusted_height = self.app_trusted_height().await?;

            if (tip_height as u32) < trusted_height {
                std::thread::sleep(std::time::Duration::from_secs(1));
            } else {
                break;
            }
        }

        Ok(())
    }

    async fn seek_to_tip(&mut self) -> Result<()> {
        let tip_height = self.get_rpc_height()?;
        let mut app_height = self.app_height().await?;

        while app_height < tip_height {
            println!("seek_to_tip: btc={}, app={}", tip_height, app_height);
            let headers = self.get_header_batch(SEEK_BATCH_SIZE).await?;

            self.app_client.peg.add(headers.into()).await?;

            app_height = self.app_height().await?;
        }
        Ok(())
    }

    async fn get_header_batch(&self, batch_size: u32) -> Result<Vec<WrappedHeader>> {
        let mut headers = Vec::with_capacity(batch_size as usize);
        for i in 1..=batch_size {
            let app_height = self.app_height().await?;

            let hash = match self.btc_client.get_block_hash((app_height + i) as u64) {
                Ok(hash) => hash,
                Err(_) => break,
            };

            let header = self.btc_client.get_block_header(&hash)?;
            let height = self.btc_client.get_block_header_info(&hash)?.height;
            let wrapped_header = WrappedHeader::from_header(&header, height as u32);
            headers.push(wrapped_header);
        }

        Ok(headers)
    }

    fn get_rpc_height(&self) -> Result<u32> {
        let tip_hash = self.btc_client.get_best_block_hash()?;
        let tip_height = self.btc_client.get_block_header_info(&tip_hash)?.height;

        Ok(tip_height as u32)
    }

    async fn step_header(&mut self) -> Result<()> {
        let tip_hash = self.btc_client.get_best_block_hash()?;
        let tip_height = self.btc_client.get_block_header_info(&tip_hash)?.height;
        let app_height = self.app_height().await?;

        println!("relayer listen: btc={}, app={}", tip_height, app_height);
        if tip_height as u32 > app_height {
            self.seek_to_tip().await?;
        } else {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        Ok(())
    }

    fn get_descriptors(&self) -> Result<Vec<ScanTxOutRequest>> {
        todo!()
    }

    fn step_transaction(&mut self) -> Result<()> {
        let descriptors = self.get_descriptors()?;
        let tx_outset = self.btc_client.scan_tx_out_set_blocking(&descriptors)?;

        let mut tx_list: Vec<DepositTxn> = Vec::new();

        for tx in tx_outset.unspents.iter() {
            if !tx.script_pub_key.is_v0_p2wsh() {
                continue;
            }

            let block_hash = self.btc_client.get_block_hash(tx.height as u64)?;
            let block = self.btc_client.get_block(&block_hash)?;
            let block_proof = MerkleBlock::from_block_with_predicate(&block, |x| x == &tx.txid).txn;
            let payable_addr = match self.listen_map.get(&tx.script_pub_key) {
                Some(pk) => pk,
                None => continue,
            };

            let transaction_result = self.btc_client.get_transaction(&tx.txid, Some(false))?;
            let transaction = transaction_result.transaction()?;
            let index = match transaction_result.info.blockindex {
                Some(index) => index,
                //not sure if should just pass through quitely here
                None => continue,
            };

            let deposit = DepositTxn {
                block_height: tx.height as u32,
                transaction: Adapter::new(transaction),
                index: index as u32,
                proof: Adapter::new(block_proof),
                payable_addr: *payable_addr,
            };

            tx_list.push(deposit);
            //some kind of call that sends this to the chain to be verified
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitcoin::adapter::Adapter;
    use crate::bitcoin::header_queue::Config;
    use crate::bitcoin::peg::Peg;
    use bitcoincore_rpc::Auth;
    use bitcoind::BitcoinD;
    use orga::encoding::Encode;
    use orga::store::{MapStore, Shared, Store};
    use std::sync::{Arc, Mutex};

    // #[test]
    // fn relayer_seek() {
    //     let bitcoind = BitcoinD::new(bitcoind::downloaded_exe_path().unwrap()).unwrap();

    //     let address = bitcoind.client.get_new_address(None, None).unwrap();
    //     bitcoind.client.generate_to_address(30, &address).unwrap();
    //     let trusted_hash = bitcoind.client.get_block_hash(30).unwrap();
    //     let trusted_header = bitcoind.client.get_block_header(&trusted_hash).unwrap();

    //     let bitcoind_url = bitcoind.rpc_url();
    //     let bitcoin_cookie_file = bitcoind.params.cookie_file.clone();
    //     let rpc_client =
    //         BtcClient::new(&bitcoind_url, Auth::CookieFile(bitcoin_cookie_file)).unwrap();

    //     let encoded_header = Encode::encode(&Adapter::new(trusted_header)).unwrap();
    //     let mut config: Config = Default::default();
    //     config.encoded_trusted_header = encoded_header;
    //     config.trusted_height = 30;
    //     config.retargeting = false;

    //     bitcoind.client.generate_to_address(100, &address).unwrap();

    //     let store = Store::new(Shared::new(MapStore::new()).into());

    //     let header_queue = HeaderQueue::with_conf(store, Default::default(), config).unwrap();

    //     let arc_mut = Arc::new(Mutex::new(Peg::new(header_queue)));
    //     let mut relayer = Relayer::new(rpc_client, arc_mut.clone());
    //     relayer.seek_to_tip().unwrap();
    //     let height = arc_mut.height().unwrap();

    //     assert_eq!(height, 130);
    // }

    // #[test]
    // fn relayer_seek_uneven_batch() {
    //     let bitcoind = BitcoinD::new(bitcoind::downloaded_exe_path().unwrap()).unwrap();

    //     let address = bitcoind.client.get_new_address(None, None).unwrap();
    //     bitcoind.client.generate_to_address(30, &address).unwrap();
    //     let trusted_hash = bitcoind.client.get_block_hash(30).unwrap();
    //     let trusted_header = bitcoind.client.get_block_header(&trusted_hash).unwrap();

    //     let bitcoind_url = bitcoind.rpc_url();
    //     let bitcoin_cookie_file = bitcoind.params.cookie_file.clone();
    //     let rpc_client =
    //         BtcClient::new(&bitcoind_url, Auth::CookieFile(bitcoin_cookie_file)).unwrap();

    //     let encoded_header = Encode::encode(&Adapter::new(trusted_header)).unwrap();
    //     let mut config: Config = Default::default();
    //     config.encoded_trusted_header = encoded_header;
    //     config.trusted_height = 30;
    //     config.retargeting = false;

    //     bitcoind
    //         .client
    //         .generate_to_address(42 as u64, &address)
    //         .unwrap();

    //     let store = Store::new(Shared::new(MapStore::new()).into());

    //     let header_queue = HeaderQueue::with_conf(store, Default::default(), config).unwrap();
    //     let arc_mut = Arc::new(Mutex::new(Peg::new(header_queue)));
    //     let mut relayer = Relayer::new(rpc_client, arc_mut.clone());
    //     relayer.seek_to_tip().unwrap();
    //     let height = arc_mut.height().unwrap();

    //     assert_eq!(height, 72);
    // }
}
