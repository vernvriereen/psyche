use std::{
    fs::File,
    io::Read,
    path::PathBuf,
    time::Instant,
};

use clap::Parser;
use flate2::{
    read::{DeflateDecoder, GzDecoder, ZlibDecoder},
    write::{DeflateEncoder, GzEncoder, ZlibEncoder},
    Compression,
};
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

macro_rules! define_compression_test_function {
    ($fn_name:ident, $encoder:ty, $decoder:ty) => {
        fn $fn_name(
            reference_data: &Vec<SerializedDistroResult>,
            original_size: usize,
            iterations: usize,
        ) -> anyhow::Result<()> {
            for level in 1..=9 {
                let mut compressed_size = 0;
                let compression = Compression::new(level);

                let start = Instant::now();
                for _ in 0..iterations {
                    // Perform the compression
                    let compressed_bytes: Vec<u8> = {
                        let mut encoder = <$encoder>::new(Vec::new(), compression.into());
                        postcard::to_io(reference_data, &mut encoder)?;
                        encoder.finish()?
                    };

                    compressed_size = compressed_bytes.len();
                }
                let duration = start.elapsed();

                // Verify round-trip works correctly
                let compressed_bytes: Vec<u8> = {
                    let mut encoder = <$encoder>::new(Vec::new(), compression.into());
                    postcard::to_io(reference_data, &mut encoder)?;
                    encoder.finish()?
                };

                let decompressed_bytes = {
                    let mut decoder = <$decoder>::new((&compressed_bytes[..]).into());
                    let mut result = Vec::new();
                    decoder.read_to_end(&mut result)?;
                    result
                };

                let decoded =
                    postcard::from_bytes::<Vec<SerializedDistroResult>>(&decompressed_bytes)?;

                assert_eq!(
                    reference_data.len(),
                    decoded.len(),
                    "number of distro results differs after round-trip at level {level}"
                );
                assert_eq!(reference_data, &decoded);

                // Calculate average duration and compression ratio
                let avg_duration = duration / iterations as u32;
                let avg_ms = avg_duration.as_micros() as f32 / 1000.0;
                let compression_ratio = original_size as f64 / compressed_size as f64;

                println!(
                    "{:<10} {:<10} {:<15.2} {:<15.2} {:<15}",
                    stringify!($fn_name),
                    level,
                    compression_ratio,
                    avg_ms,
                    compressed_size
                );
            }

            Ok(())
        }
    };
}

define_compression_test_function!(zlib, ZlibEncoder<Vec<u8>>, ZlibDecoder<&[u8]>);
define_compression_test_function!(gzip, GzEncoder<Vec<u8>>, GzDecoder<&[u8]>);
define_compression_test_function!(deflate, DeflateEncoder<Vec<u8>>, DeflateDecoder<&[u8]>);

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
    // Test all compression types across levels 1 through 9
    println!("Compression Results:");
    println!(
        "{:<10} {:<10} {:<15} {:<15} {:<15}",
        "Algorithm", "Level", "Ratio", "Time (ms)", "Size (bytes)"
    );
    println!("{}", "-".repeat(65));

    // Run tests for each encoder type
    zlib(&reference_distro_results, original_size, args.iterations)?;
    gzip(&reference_distro_results, original_size, args.iterations)?;
    deflate(&reference_distro_results, original_size, args.iterations)?;

    Ok(())
}
