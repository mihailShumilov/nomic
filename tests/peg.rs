use nomic::app::App;
use nomic::app::InnerApp;
use nomic::bitcoin::peg::Peg;
use orga::prelude::*;
use std::thread;

pub fn app_client() -> TendermintClient<App> {
    TendermintClient::new("http://localhost:26657").unwrap()
}

#[tokio::test]
async fn peg_query() {
    thread::spawn(move || Node::<App>::new(".peg-integration").reset().run());
    thread::sleep(std::time::Duration::from_secs(2));

    let client = app_client();
    type AppQuery = <InnerApp as Query>::Query;
    type PegQuery = <Peg as Query>::Query;

    let query = AppQuery::FieldPeg(PegQuery::MethodHeight(vec![]));
    let height: u64 = client
        .query(query, |state| state.peg.height())
        .await
        .unwrap()
        .into();

    assert_eq!(height, 0);
}
