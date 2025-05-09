import os
import sys
import subprocess
import multiprocessing
from concurrent.futures import ThreadPoolExecutor, as_completed
import argparse
import json
import threading
import time


def process_file(input_file, n_value, s_value):
    cmd = f"cargo run --quiet --release -p expand-distro < {input_file} | go run ../go-tools/cmd/chunker/main.go -N {n_value} -S {s_value}"

    try:
        result = subprocess.run(
            cmd,
            shell=True,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

        # Assuming the output is valid JSON
        data = json.loads(result.stdout)

        return {"Path": input_file, "Data": data}
    except subprocess.CalledProcessError as e:
        print(f"Error processing {input_file}: {e.stderr}", file=sys.stderr)
        return None


def print_progress(total, current, bar_length=50):
    fraction = current / total
    filled_length = int(bar_length * fraction)
    bar = "#" * filled_length + "-" * (bar_length - filled_length)
    percent = fraction * 100
    sys.stdout.write(f"\rProgress: |{bar}| {percent:.1f}% ({current}/{total})")
    sys.stdout.flush()


def main(input_dir, output_file, n_value, s_value):
    # Validate input directory
    if not os.path.isdir(input_dir):
        print(f"Error: {input_dir} is not a valid directory.", file=sys.stderr)
        sys.exit(1)

    # Gather all .vec-postcard files
    files_to_process = [
        os.path.join(input_dir, f)
        for f in os.listdir(input_dir)
        if f.endswith(".vec-postcard")
    ]

    total_files = len(files_to_process)

    if total_files == 0:
        print(f"No .vec-postcard files found in {input_dir}.", file=sys.stderr)
        sys.exit(1)

    max_threads = min(23, multiprocessing.cpu_count())
    aggregated_data = []
    processed_count = 0
    lock = threading.Lock()

    print_progress(total_files, processed_count)

    with ThreadPoolExecutor(max_workers=max_threads) as executor:
        futures = [
            executor.submit(process_file, f, n_value, s_value) for f in files_to_process
        ]

        for future in as_completed(futures):
            result = future.result()
            if result:
                aggregated_data.append(result)
            with lock:
                processed_count += 1
                print_progress(total_files, processed_count)

    print()

    # Write the aggregated data to the output JSON file
    try:
        with open(output_file, "w") as outfile:
            json.dump(aggregated_data, outfile, indent=2)
        print(f"Aggregated JSON data has been written to {output_file}.")
    except IOError as e:
        print(f"Error writing to {output_file}: {str(e)}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Expand, chunk, and aggregate JSON files into a single JSON file."
    )
    parser.add_argument(
        "input_directory", help="Directory containing input .json.gz files"
    )
    parser.add_argument("output_file", help="Path to the output aggregated JSON file")
    parser.add_argument(
        "--N", type=int, default=64000, help="Chunk size (default: 64000)"
    )
    parser.add_argument("--S", type=int, default=0, help="Randomness salt (default: 0)")

    args = parser.parse_args()

    main(args.input_directory, args.output_file, args.N, args.S)
