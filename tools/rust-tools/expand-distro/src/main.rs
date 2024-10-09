use std::{
    env,
    io::{self, Write},
};

use anyhow::bail;
use psyche_client::distro_results_from_reader;
use psyche_modeling::{CompressDCT, DistroResult};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 1 {
        bail!("Usage: cat <postcard_file> | expand-distro");
    }

    let target_type = tch::Kind::BFloat16;
    let target_device = tch::Device::cuda_if_available();

    let distro_results_iter = distro_results_from_reader(io::stdin());

    for serialized_result in distro_results_iter {
        let result: DistroResult = (&serialized_result?).try_into()?;

        let decompressed = CompressDCT::decompress(
            &result.sparse_idx,
            &result.sparse_val,
            &result.xshape,
            target_type,
            target_device,
        );

        let flat: Vec<f32> = (&decompressed.flatten(0, -1)).try_into()?;
        let bytes = flat.into_iter().map(|f| f.to_le_bytes());
        for byte in bytes {
            std::io::stdout().write_all(&byte)?;
        }
    }
    Ok(())
}
