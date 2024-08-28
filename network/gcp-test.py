import subprocess
import time
import signal
import sys
import argparse
import uuid

# List to store container IDs
containers = []


def cleanup():
    print("\nCleaning up containers...")
    for container in containers:
        subprocess.run(
            [
                "gcloud",
                "compute",
                "instances",
                "delete",
                container,
                "--project",
                args.project_id,
                "--quiet",
                "--zone",
                "us-central1-a",
            ]
        )
    print("All containers terminated.")


def signal_handler(sig, frame):
    print("\nScript interrupted. Cleaning up...")
    cleanup()
    sys.exit(0)


# Register the signal handler
signal.signal(signal.SIGINT, signal_handler)
signal.signal(signal.SIGTERM, signal_handler)

# Parse command-line arguments
parser = argparse.ArgumentParser(description="Spin up Docker containers on GCP")
parser.add_argument("project_id", help="GCP project ID")
parser.add_argument("num_containers", type=int, help="Number of containers to spin up")
parser.add_argument("container_image", help="Docker image to use")
parser.add_argument(
    "--runtime", type=int, default=60, help="Runtime in seconds (default: 60)"
)
args = parser.parse_args()

run_id = str(uuid.uuid4())[:8]
try:
    # Spin up N containers
    start_threads = []
    for i in range(args.num_containers):
        container_name = f"{run_id}-container-{i}"
        print(f"Starting container {container_name}...")
        containers.append(container_name)
        p = subprocess.Popen(
            [
                "gcloud",
                "compute",
                "instances",
                "create-with-container",
                container_name,
                "--project",
                args.project_id,
                "--container-image",
                args.container_image,
                "--machine-type",
                "n1-standard-1",
                "--zone",
                "us-central1-a",
            ],
        )
        start_threads.append(p)

    for thread in start_threads:
        result = thread.wait()
        if result.returncode == 0:
            print(f"Container {container_name} started successfully.")
        else:
            containers.remove(container_name)
            print(f"Failed to start container {container_name}. Error: {result.stderr}")

    # Wait for specified time with progress bar
    print(f"Waiting for {args.runtime} seconds...")
    for i in range(args.runtime):
        print(f"{i}/{args.runtime}...")
        time.sleep(1)

finally:
    # Cleanup
    cleanup()

print("Script completed successfully.")
