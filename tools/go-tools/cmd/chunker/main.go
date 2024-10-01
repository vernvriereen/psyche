package main

import (
	"bufio"
	"encoding/binary"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"math/rand"
	"os"
)

func deterministicStreamingSample(reader io.Reader, N int, seed int64) ([]float64, error) {
	r := rand.New(rand.NewSource(seed))
	result := make([]float64, 0, N)
	var f float32
	var count int64 = 0

	bufReader := bufio.NewReader(reader)

	for {
		err := binary.Read(bufReader, binary.LittleEndian, &f)
		if err == io.EOF {
			break
		}
		if err != nil {
			return nil, err
		}
		count++

		if len(result) < N {
			result = append(result, float64(f))
		} else {
			j := r.Int63n(count)
			if j < int64(N) {
				result[j] = float64(f)
			}
		}
	}

	return result, nil
}

func main() {
	N := flag.Int("N", 0, "The number of elements to sample")
	S := flag.Int("S", 0, "The seed for the random number generator")
	flag.Parse()

	if *N <= 0 {
		fmt.Fprintln(os.Stderr, "N must be a positive integer")
		os.Exit(1)
	}

	// Read and sample binary float32 data from stdin
	sampledData, err := deterministicStreamingSample(os.Stdin, *N, int64(*S))
	if err != nil {
		fmt.Fprintln(os.Stderr, "Error processing input data:", err)
		os.Exit(1)
	}

	// Write the sampled data as JSON to stdout
	encoder := json.NewEncoder(os.Stdout)
	encoder.SetIndent("", "  ")
	if err := encoder.Encode(sampledData); err != nil {
		fmt.Fprintln(os.Stderr, "Error encoding JSON:", err)
		os.Exit(1)
	}
}
