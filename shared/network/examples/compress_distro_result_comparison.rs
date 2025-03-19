use std::{fs::File, io::Read, path::PathBuf, time::Instant};

use clap::Parser;
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use psyche_network::{distro_results_from_reader, SerializedDistroResult};

/// Command line arguments for the compression benchmark program
#[derive(Parser, Debug)]
#[clap(
    author,
    version,
    about = "Benchmark compression levels for distro results"
)]
struct Args {
    /// Path to the distro results postcard file
    postcard_file: PathBuf,

    /// Number of iterations for each compression level
    #[clap(short, long, default_value = "5")]
    iterations: usize,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Read the whole file into memory as distro results
    let reference_distro_results: Vec<SerializedDistroResult> = {
        let file = File::open(&args.postcard_file)?;
        let reader = distro_results_from_reader(file);
        let data: Result<Vec<_>, _> = reader.collect();
        data?
    };

    let reference_serialized_bytes: Vec<u8> = postcard::to_allocvec(&reference_distro_results)?;
    assert_eq!(
        reference_distro_results,
        postcard::from_bytes::<Vec<SerializedDistroResult>>(&reference_serialized_bytes)?,
        "failed to roundtrip distro results."
    );

    let original_size = reference_serialized_bytes.len();

    println!("Original data size: {} bytes", original_size);
    println!(
        "Running {} iterations for each compression level",
        args.iterations
    );
    println!(
        "\n{:<10} {:<15} {:<15} {:<15}",
        "Level", "Comp Ratio", "Time (ms)", "Size (bytes)"
    );
    println!("{:-<55}", "");

    // Test compression levels 1 through 9
    for level in 1..=9 {
        let mut compressed_size = 0;
        let compression = Compression::new(level);

        let start = Instant::now();
        for _ in 0..args.iterations {
            // Perform the compression / decompression
            let compressed_bytes: Vec<u8> = {
                let mut encoder = ZlibEncoder::new(Vec::new(), compression);
                postcard::to_io(&reference_distro_results, &mut encoder)?;
                encoder.finish()?
            };

            compressed_size = compressed_bytes.len();
        }
        let duration = start.elapsed();

        // assert it actually round-tripped
        let compressed_bytes: Vec<u8> = {
            let mut encoder = ZlibEncoder::new(Vec::new(), compression);
            postcard::to_io(&reference_distro_results, &mut encoder)?;
            encoder.finish()?
        };

        let decompressed_bytes = {
            let mut decoder = ZlibDecoder::new(&compressed_bytes[..]);
            let mut result = Vec::new();
            decoder.read_to_end(&mut result)?;
            result
        };

        let decoded = postcard::from_bytes::<Vec<SerializedDistroResult>>(&decompressed_bytes)?;

        assert_eq!(
            reference_distro_results.len(),
            decoded.len(),
            "number of distro results differs after round-trip at level {level}"
        );
        assert_eq!(reference_distro_results, decoded);

        // Calculate average duration and compression ratio
        let avg_duration = duration / args.iterations as u32;
        let avg_ms = avg_duration.as_micros() as f32 / 1000.0;
        let compression_ratio = original_size as f64 / compressed_size as f64;

        println!(
            "{:<10} {:<15.2} {:<15} {:<15}",
            level, compression_ratio, avg_ms, compressed_size
        );
    }

    Ok(())
}
