use std::io::{self, Write};

use clap::Parser;
use psyche_client::distro_results_from_reader;
use psyche_modeling::{CompressDCT, DistroResult};

#[derive(Parser, Debug)]
struct Args {
    #[clap(long, default_value_t = false)]
    cpu: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let target_type = tch::Kind::BFloat16;
    let target_device = if args.cpu {
        tch::Device::Cpu
    } else {
        tch::Device::cuda_if_available()
    };

    let distro_results_iter = distro_results_from_reader(io::stdin());

    for serialized_result in distro_results_iter {
        let mut result: DistroResult = (&serialized_result?).try_into()?;
        result.sparse_idx = result.sparse_idx.to_device(target_device);
        result.sparse_val = result.sparse_val.to_device(target_device);
        let decompressed = CompressDCT::decompress(
            &result.sparse_idx,
            &result.sparse_val,
            &result.xshape,
            result.totalk,
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
