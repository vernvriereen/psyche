import os
import re
import sys


def rename_files(directory):
    for filename in os.listdir(directory):
        if filename.endswith((".json.gz", ".json")):
            full_path = os.path.join(directory, filename)
            new_filename = re.sub(
                r"round-(\d+)", lambda x: f"round-{int(x.group(1)):04d}", filename
            )
            if new_filename != filename:
                print(f"Renaming: {filename} -> {new_filename}")
                os.rename(full_path, os.path.join(directory, new_filename))


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("Usage: python3 fix-names.py <directory>")
    else:
        directory = sys.argv[1]
        if os.path.isdir(directory):
            rename_files(directory)
        else:
            print(f"Error: {directory} is not a valid directory")
