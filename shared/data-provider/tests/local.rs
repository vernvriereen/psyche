use std::path::PathBuf;

use pretty_assertions::assert_eq;
use psyche_data_provider::{LocalDataProvider, TokenSize, TokenizedDataProvider};
use tokenizers::Tokenizer;
use tokio::{
    fs::{read_to_string, File},
    io::AsyncWriteExt,
};
fn test_path(path: &[&str]) -> PathBuf {
    [env!("CARGO_MANIFEST_DIR"), "tests"]
        .iter()
        .chain(path)
        .collect()
}

const SEED: [u8; 32] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26,
    27, 28, 29, 30, 31, 32,
];

#[tokio::test]
async fn loads_dolma_subset() {
    let data_dir = test_path(&["resources", "dolma", "data"]);
    let mut data_loader =
        LocalDataProvider::new_from_directory(data_dir, TokenSize::TwoBytes, 2048, SEED).unwrap();
    let samples = data_loader.get_samples(&[0, 1]).await.unwrap();

    let tokenizer = Tokenizer::from_file(test_path(&["resources", "llama2_tokenizer.json"]))
        .expect("tokenizer json exists");
    for (i, sample) in samples.into_iter().enumerate() {
        let decoded_path = test_path(&["resources", "dolma", "decoded", &format!("{}.txt", i)]);

        let expected = read_to_string(&decoded_path)
            .await
            .expect(&format!("no decoded file at {decoded_path:?}"));

        let decoded = tokenizer
            .decode(
                &sample.into_iter().map(|x| x as u32).collect::<Vec<_>>(),
                true,
            )
            .unwrap();

        assert_eq!(
            decoded, expected,
            "sample {i} (left) doesn't match decoded reference (right) from file {decoded_path:?}"
        );
    }
}

#[tokio::test]
async fn loads_fineweb_subset() {
    let data_dir = test_path(&["resources", "fineweb", "data"]);
    let mut data_loader =
        LocalDataProvider::new_from_directory(data_dir, TokenSize::TwoBytes, 2048, SEED).unwrap();
    let samples = data_loader.get_samples(&[0, 1]).await.unwrap();

    let tokenizer = Tokenizer::from_file(test_path(&["resources", "llama2_tokenizer.json"]))
        .expect("tokenizer json exists");
    for (i, sample) in samples.into_iter().enumerate() {
        let decoded_path = test_path(&["resources", "fineweb", "decoded", &format!("{}.txt", i)]);

        let expected = read_to_string(&decoded_path)
            .await
            .expect(&format!("no decoded file at {decoded_path:?}"));

        let decoded = tokenizer
            .decode(
                &sample.into_iter().map(|x| x as u32).collect::<Vec<_>>(),
                true,
            )
            .unwrap();

        assert_eq!(
            decoded, expected,
            "sample {i} (left) doesn't match decoded reference (right) from file {decoded_path:?}"
        );
    }
}

#[allow(unused)]
async fn write_to_tmp_file(data: &[u8]) {
    let mut f = File::create("/tmp/foo")
        .await
        .expect("Unable to create /tmp/foo");
    f.write_all(data).await.expect("Unable to write data");
    println!("wrote data to /tmp/foo")
}
