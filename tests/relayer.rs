use bitcoincore_rpc::{Auth, Client as BtcClient, RpcApi};
use bitcoind::BitcoinD;
use nomic::app::App;
use nomic::bitcoin::relayer::Relayer;
use orga::prelude::*;
use std::thread;

pub fn app_client() -> TendermintClient<App> {
    TendermintClient::new("http://localhost:26657").unwrap()
}

#[tokio::test]
async fn relayer() {
    let bitcoind = BitcoinD::new(bitcoind::downloaded_exe_path().unwrap()).unwrap();

    let bitcoind_url = bitcoind.rpc_url();
    let bitcoin_cookie_file = bitcoind.params.cookie_file.clone();
    let rpc_client = BtcClient::new(&bitcoind_url, Auth::CookieFile(bitcoin_cookie_file)).unwrap();

    thread::spawn(move || Node::<App>::new(".relayer-integration").reset().run());

    thread::sleep(std::time::Duration::from_secs(2));

    let app_client = app_client();
    let relayer = Relayer::new(rpc_client, app_client);

    let height = relayer.app_trusted_height().await.unwrap();
    assert_eq!(height, 0);
}
