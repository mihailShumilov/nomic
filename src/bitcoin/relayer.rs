use crate::app::InnerApp;
use crate::bitcoin::header_queue::{HeaderList, HeaderQueue, WrappedHeader};
use crate::error::Result;
use bitcoincore_rpc::{Client as BtcClient, RpcApi};
use orga::prelude::*;

const SEEK_BATCH_SIZE: u32 = 10;

type AppClient = TendermintClient<crate::app::App>;

pub trait PegClient {
    fn height(&self) -> Result<u32>;
    fn trusted_height(&self) -> Result<u32>;
    fn add(&mut self, header: HeaderList) -> Result<()>;
}

// impl PegClient for AppClient {
//      fn height(&self) -> OrgaResult<u32> {
//         self.app_client
//             .query(
//                 AppQuery::FieldBtcHeaders(HeaderQueueQuery::MethodHeight(vec![])),
//                 |state| Ok(state.btc_headers.height().unwrap()),
//             )
//
//     }

//      fn trusted_height(&self) -> OrgaResult<u32> {
//         self.app_client
//             .query(
//                 AppQuery::FieldBtcHeaders(HeaderQueueQuery::MethodTrustedHeight(vec![])),
//                 |state| Ok(state.btc_headers.trusted_height()),
//             )
//
//     }

//      fn add(&mut self, headers: HeaderList) -> OrgaResult<()> {
//         self.app_client.btc_headers.add(headers)
//     }
// }

pub struct Relayer<P: PegClient> {
    btc_client: BtcClient,
    peg_client: P,
}

type AppQuery = <InnerApp as Query>::Query;
type HeaderQueueQuery = <HeaderQueue as Query>::Query;

impl<P: PegClient> Relayer<P> {
    pub fn new(btc_client: BtcClient, peg_client: P) -> Self {
        Relayer {
            btc_client,
            peg_client,
        }
    }

    fn listen(&mut self, func: &dyn Fn(&mut Self) -> Result<()>) -> Result<!> {
        loop {
            func(self)?;
        }
    }

    #[cfg(test)]
    fn bounded_listen(
        &mut self,
        func: &mut dyn FnMut(&mut Self) -> Result<()>,
        num_blocks: u32,
    ) -> Result<()> {
        for _ in 0..num_blocks {
            func(self)?;
        }

        Ok(())
    }

    pub fn start(&mut self) -> Result<!> {
        self.wait_for_trusted_header()?;
        self.listen(&Relayer::step_header)
    }

    fn wait_for_trusted_header(&self) -> Result<()> {
        loop {
            let tip_hash = self.btc_client.get_best_block_hash()?;
            let tip_height = self.btc_client.get_block_header_info(&tip_hash)?.height;
            println!("wait_for_trusted_header: btc={}", tip_height);
            if (tip_height as u32) < self.peg_client.trusted_height()? {
                std::thread::sleep(std::time::Duration::from_secs(1));
            } else {
                break;
            }
        }

        Ok(())
    }

    fn seek_to_tip(&mut self) -> Result<()> {
        let tip_height = self.get_rpc_height()?;
        let mut app_height = self.peg_client.height()?;
        while app_height < tip_height {
            println!("seek_to_tip: btc={}, app={}", tip_height, app_height);
            let headers = self.get_header_batch(SEEK_BATCH_SIZE)?;
            self.peg_client.add(headers.into())?;
            app_height = self.peg_client.height()?;
        }
        Ok(())
    }

    fn get_header_batch(&self, batch_size: u32) -> Result<Vec<WrappedHeader>> {
        let mut headers = Vec::with_capacity(batch_size as usize);
        for i in 1..=batch_size {
            let hash = match self
                .btc_client
                .get_block_hash((self.peg_client.height()? + i) as u64)
            {
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

    fn step_header(&mut self) -> Result<()> {
        let tip_hash = self.btc_client.get_best_block_hash()?;
        let tip_height = self.btc_client.get_block_header_info(&tip_hash)?.height;
        println!(
            "relayer listen: btc={}, app={}",
            tip_height,
            self.peg_client.height()?
        );
        if tip_height as u32 > self.peg_client.height()? {
            self.seek_to_tip()?;
        } else {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitcoin::adapter::Adapter;
    use crate::bitcoin::header_queue::Config;
    use bitcoincore_rpc::Auth;
    use bitcoind::BitcoinD;
    use orga::encoding::Encode;
    use orga::store::{MapStore, Shared, Store};
    use std::sync::{Arc, Mutex};

    impl PegClient for Arc<Mutex<HeaderQueue>> {
        fn height(&self) -> Result<u32> {
            self.lock().unwrap().height()
        }

        fn trusted_height(&self) -> Result<u32> {
            Ok(self.lock().unwrap().trusted_height())
        }

        fn add(&mut self, headers: HeaderList) -> Result<()> {
            self.lock().unwrap().add_into_iter(headers)
        }
    }

    #[test]
    fn relayer_seek() {
        let bitcoind = BitcoinD::new(bitcoind::downloaded_exe_path().unwrap()).unwrap();

        let address = bitcoind.client.get_new_address(None, None).unwrap();
        bitcoind.client.generate_to_address(30, &address).unwrap();
        let trusted_hash = bitcoind.client.get_block_hash(30).unwrap();
        let trusted_header = bitcoind.client.get_block_header(&trusted_hash).unwrap();

        let bitcoind_url = bitcoind.rpc_url();
        let bitcoin_cookie_file = bitcoind.params.cookie_file.clone();
        let rpc_client =
            BtcClient::new(&bitcoind_url, Auth::CookieFile(bitcoin_cookie_file)).unwrap();

        let encoded_header = Encode::encode(&Adapter::new(trusted_header)).unwrap();
        let mut config: Config = Default::default();
        config.encoded_trusted_header = encoded_header;
        config.trusted_height = 30;
        config.retargeting = false;

        bitcoind.client.generate_to_address(100, &address).unwrap();

        let store = Store::new(Shared::new(MapStore::new()).into());

        let header_queue = HeaderQueue::with_conf(store, Default::default(), config).unwrap();

        let arc_mut = Arc::new(Mutex::new(header_queue));
        let mut relayer = Relayer::new(rpc_client, arc_mut.clone());
        relayer.seek_to_tip().unwrap();
        let height = arc_mut.lock().unwrap().height().unwrap();

        assert_eq!(height, 130);
    }

    #[test]
    fn relayer_seek_uneven_batch() {
        let bitcoind = BitcoinD::new(bitcoind::downloaded_exe_path().unwrap()).unwrap();

        let address = bitcoind.client.get_new_address(None, None).unwrap();
        bitcoind.client.generate_to_address(30, &address).unwrap();
        let trusted_hash = bitcoind.client.get_block_hash(30).unwrap();
        let trusted_header = bitcoind.client.get_block_header(&trusted_hash).unwrap();

        let bitcoind_url = bitcoind.rpc_url();
        let bitcoin_cookie_file = bitcoind.params.cookie_file.clone();
        let rpc_client =
            BtcClient::new(&bitcoind_url, Auth::CookieFile(bitcoin_cookie_file)).unwrap();

        let encoded_header = Encode::encode(&Adapter::new(trusted_header)).unwrap();
        let mut config: Config = Default::default();
        config.encoded_trusted_header = encoded_header;
        config.trusted_height = 30;
        config.retargeting = false;

        bitcoind
            .client
            .generate_to_address(42 as u64, &address)
            .unwrap();

        let store = Store::new(Shared::new(MapStore::new()).into());

        let header_queue = HeaderQueue::with_conf(store, Default::default(), config).unwrap();
        let arc_mut = Arc::new(Mutex::new(header_queue));
        let mut relayer = Relayer::new(rpc_client, arc_mut.clone());
        relayer.seek_to_tip().unwrap();
        let height = arc_mut.lock().unwrap().height().unwrap();

        assert_eq!(height, 72);
    }
}
