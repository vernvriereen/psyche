use psyche_decentralized_testing::utils::SolanaTestClient;

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn get_state() {
    let run_id = String::from("test");
    let client = SolanaTestClient::new(run_id).await;
    println!("state: {:?}", client.get_run_state().await);
}
