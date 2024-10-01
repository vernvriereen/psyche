#!/usr/bin/env python3

import pandas as pd
import re
import matplotlib.pyplot as plt
import seaborn as sns
import argparse
import os
import sys

def extract_data_value(filename):
    match = re.search(r'data-([^-]+)-distro', filename)
    if match:
        return match.group(1)
    else:
        return None

def process_csv(csv_path):
    try:
        df = pd.read_csv(csv_path)
    except Exception as e:
        print(f"Error reading CSV file: {e}")
        sys.exit(1)
    
    if 'File' not in df.columns:
        print("Error: CSV file must have a 'File' column as the first column.")
        sys.exit(1)
    
    data_columns = df.columns[1:]
    data_values = {col: extract_data_value(col) for col in data_columns}
    
    if None in data_values.values():
        print("Warning: Some filenames did not match the expected pattern and were skipped.")
    
    same_data_cmp = []
    different_data_cmp = []
    
    for index, row in df.iterrows():
        file1 = row['File']
        data1 = extract_data_value(file1)
        
        if data1 is None:
            continue
        
        for col in data_columns:
            file2 = col
            data2 = data_values.get(col, None)
            cmp_value = row[col]
            
            if data2 is None:
                continue
            
            if file1 == file2:
                continue
            
            if data1 == data2:
                same_data_cmp.append(cmp_value)
            else:
                different_data_cmp.append(cmp_value)
    
    same_data_series = pd.Series(same_data_cmp, name='Same Data')
    different_data_series = pd.Series(different_data_cmp, name='Different Data')
    
    print(f"Total same data comparisons: {len(same_data_series)}")
    print(f"Total different data comparisons: {len(different_data_series)}")
    
    return same_data_series, different_data_series

def generate_plot(same_data, different_data, output_path, title):
    plt.figure(figsize=(12, 6))
    
    sns.histplot(same_data, color='blue', label='Same Data', kde=True, stat="density", bins=50, alpha=0.6)
    sns.histplot(different_data, color='red', label='Different Data', kde=True, stat="density", bins=50, alpha=0.6)
    
    plt.legend()
    plt.title(title)
    plt.xlabel('CMP Value')
    plt.ylabel('Density')
    plt.tight_layout()
    
    try:
        plt.savefig(output_path)
        print(f"Plot saved to {output_path}")
    except Exception as e:
        print(f"Error saving plot: {e}")
    finally:
        plt.close()

def main():
    parser = argparse.ArgumentParser(description='Visualize CMP comparison matrix.')
    parser.add_argument('csv_file', type=str, help='Path to the CSV comparison matrix file.')
    parser.add_argument('--suffix', type=str, default='_cmp_distribution.png',
                        help='Suffix for the output graph file (default: _cmp_distribution.png)')
    args = parser.parse_args()
    
    csv_path = args.csv_file
    
    if not os.path.isfile(csv_path):
        print(f"Error: File '{csv_path}' does not exist.")
        sys.exit(1)
    
    same_data, different_data = process_csv(csv_path)

    base, ext = os.path.splitext(csv_path)
    output_path = f"{base}{args.suffix}"
    title = os.path.basename(csv_path)

    generate_plot(same_data, different_data, output_path, title)

if __name__ == "__main__":
    main()
