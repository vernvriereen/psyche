package main

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"flag"
	"fmt"
	"io/ioutil"
	"math"
	"os"
	"runtime"
	"sort"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

type Result struct {
	FileName string   `json:"file_name"`
	MinHash  []uint64 `json:"min_hash"`
}

type LoadedFile struct {
	Path    string
	Data    []float64
	Round   int
	Missing bool
}

type Output struct {
	P       float64  `json:"P"`
	N       int      `json:"N"`
	B       int      `json:"B"`
	Results []Result `json:"results"`
}

func norm(x float64, p float64) float64 {
	sgn := math.Copysign(1, x)
	innerFunction := (sgn * math.Log(math.Abs(x)+1)) / -0.3
	floorFunction := math.Floor(innerFunction / p)
	result := floorFunction * p
	return result
}

func applyNorm(nestedList []float64, p float64) {
	for i, x := range nestedList {
		nestedList[i] = norm(x, p)
	}
}

func CmpFilesCanberra(a, b *LoadedFile) float64 {
	sum := 0.0
	for i := range a.Data {
		num := math.Abs(a.Data[i] - b.Data[i])
		denom := math.Abs(a.Data[i]) + math.Abs(b.Data[i])
		if denom != 0 {
			sum += num / denom
		}
	}
	return sum
}

// Updated CmpFiles function with new methods
func CmpFiles(a, b *LoadedFile, method string) float64 {
	switch method {
	case "jaccard":
		return CmpFilesJaccard(a, b)
	case "euclidean":
		return CmpFilesEuclidean(a, b)
	case "cosine":
		return CmpFilesCosine(a, b)
	case "pearson":
		return CmpFilesPearson(a, b)
	case "manhattan":
		return CmpFilesManhattan(a, b)
	case "chebyshev":
		return CmpFilesChebyshev(a, b)
	case "canberra":
		return CmpFilesCanberra(a, b)
	case "braycurtis":
		return CmpFilesBrayCurtis(a, b)
	case "minkowski":
		return CmpFilesMinkowski(a, b, 3) // example with p=3
	case "hamming":
		return CmpFilesHamming(a, b)
	case "chiSquare":
		return CmpFilesChiSquare(a, b)
	case "klDivergence":
		return CmpFilesKLDivergence(a, b)
	case "bhattacharyya":
		return CmpFilesBhattacharyya(a, b)
	case "mahalanobis":
		return CmpFilesMahalanobis(a, b)
	case "spearman":
		return CmpFilesSpearman(a, b)
	case "kendall":
		return CmpFilesKendall(a, b)
	case "tanimoto":
		return CmpFilesTanimoto(a, b)
	case "overlap":
		return CmpFilesOverlap(a, b)
	case "hausdorff":
		return CmpFilesHausdorff(a, b)
	case "dynamicTimeWarping":
		return CmpFilesDynamicTimeWarping(a, b)
	case "editDistance":
		return CmpFilesEditDistance(a, b)
	case "tversky":
		return CmpFilesTversky(a, b)
	default:
		panic("Unsupported method")
	}
}

func CmpFilesTversky(a, b *LoadedFile) float64 {
	alpha, beta := 0.5, 0.5
	intersection, onlyA, onlyB := 0.0, 0.0, 0.0
	for i := range a.Data {
		if a.Data[i] == b.Data[i] {
			intersection++
		} else {
			onlyA++
			onlyB++
		}
	}
	return intersection / (intersection + alpha*onlyA + beta*onlyB)
}

func CmpFilesEditDistance(a, b *LoadedFile) float64 {
	n, m := len(a.Data), len(b.Data)
	dp := make([][]int, n+1)
	for i := range dp {
		dp[i] = make([]int, m+1)
	}

	for i := 0; i <= n; i++ {
		for j := 0; j <= m; j++ {
			if i == 0 {
				dp[i][j] = j
			} else if j == 0 {
				dp[i][j] = i
			} else if a.Data[i-1] == b.Data[j-1] {
				dp[i][j] = dp[i-1][j-1]
			} else {
				dp[i][j] = 1 + min(dp[i-1][j], dp[i][j-1], dp[i-1][j-1])
			}
		}
	}

	return float64(dp[n][m])
}

func CmpFilesDynamicTimeWarping(a, b *LoadedFile) float64 {
	n, m := len(a.Data), len(b.Data)
	dtw := make([][]float64, n+1)
	for i := range dtw {
		dtw[i] = make([]float64, m+1)
		for j := range dtw[i] {
			dtw[i][j] = math.Inf(1)
		}
	}
	dtw[0][0] = 0

	for i := 1; i <= n; i++ {
		for j := 1; j <= m; j++ {
			cost := math.Abs(a.Data[i-1] - b.Data[j-1])
			dtw[i][j] = cost + math.Min(dtw[i-1][j], math.Min(dtw[i][j-1], dtw[i-1][j-1]))
		}
	}

	return dtw[n][m]
}

func CmpFilesHausdorff(a, b *LoadedFile) float64 {
	maxMinDistA := 0.0
	for _, pointA := range a.Data {
		minDist := math.Inf(1)
		for _, pointB := range b.Data {
			dist := math.Abs(pointA - pointB)
			if dist < minDist {
				minDist = dist
			}
		}
		if minDist > maxMinDistA {
			maxMinDistA = minDist
		}
	}

	maxMinDistB := 0.0
	for _, pointB := range b.Data {
		minDist := math.Inf(1)
		for _, pointA := range a.Data {
			dist := math.Abs(pointA - pointB)
			if dist < minDist {
				minDist = dist
			}
		}
		if minDist > maxMinDistB {
			maxMinDistB = minDist
		}
	}

	return math.Max(maxMinDistA, maxMinDistB)
}

func CmpFilesOverlap(a, b *LoadedFile) float64 {
	minIntersection := 0.0
	for i := range a.Data {
		if a.Data[i] == b.Data[i] {
			minIntersection++
		}
	}
	return minIntersection / math.Min(float64(len(a.Data)), float64(len(b.Data)))
}

func CmpFilesTanimoto(a, b *LoadedFile) float64 {
	intersection, union := 0.0, 0.0
	for i := range a.Data {
		if a.Data[i] == b.Data[i] {
			intersection++
		}
		union++
	}
	return intersection / union
}

func CmpFilesKendall(a, b *LoadedFile) float64 {
	n := len(a.Data)
	concordant, discordant := 0.0, 0.0
	for i := 0; i < n-1; i++ {
		for j := i + 1; j < n; j++ {
			if (a.Data[i]-a.Data[j])*(b.Data[i]-b.Data[j]) > 0 {
				concordant++
			} else {
				discordant++
			}
		}
	}
	return (concordant - discordant) / (concordant + discordant)
}

func varCov(dataA, dataB []float64) float64 {
	meanA := mean(dataA)
	meanB := mean(dataB)
	varCov := 0.0
	for i := range dataA {
		varCov += (dataA[i] - meanA) * (dataB[i] - meanB)
	}
	return varCov / float64(len(dataA)-1)
}

func invertMatrix(matrix float64) float64 {
	return 1 / matrix
}

func CmpFilesMahalanobis(a, b *LoadedFile) float64 {
	meanA := mean(a.Data)
	meanB := mean(b.Data)
	diff := meanA - meanB
	varCovMatrix := varCov(a.Data, b.Data)
	invVarCovMatrix := invertMatrix(varCovMatrix)
	mahalanobisDistance := math.Sqrt(diff * invVarCovMatrix * diff)
	return mahalanobisDistance
}

func CmpFilesBhattacharyya(a, b *LoadedFile) float64 {
	sum := 0.0
	for i := range a.Data {
		sum += math.Sqrt(a.Data[i] * b.Data[i])
	}
	return -math.Log(sum)
}

func CmpFilesKLDivergence(a, b *LoadedFile) float64 {
	sum := 0.0
	for i := range a.Data {
		if a.Data[i] != 0 && b.Data[i] != 0 {
			sum += a.Data[i] * math.Log(a.Data[i]/b.Data[i])
		}
	}
	return sum
}

func CmpFilesChiSquare(a, b *LoadedFile) float64 {
	sum := 0.0
	for i := range a.Data {
		if a.Data[i]+b.Data[i] != 0 {
			diff := a.Data[i] - b.Data[i]
			sum += (diff * diff) / (a.Data[i] + b.Data[i])
		}
	}
	return sum
}

func CmpFilesMinkowski(a, b *LoadedFile, p float64) float64 {
	sum := 0.0
	for i := range a.Data {
		sum += math.Pow(math.Abs(a.Data[i]-b.Data[i]), p)
	}
	return math.Pow(sum, 1/p)
}

func CmpFilesBrayCurtis(a, b *LoadedFile) float64 {
	num := 0.0
	denom := 0.0
	for i := range a.Data {
		num += math.Abs(a.Data[i] - b.Data[i])
		denom += math.Abs(a.Data[i] + b.Data[i])
	}
	return num / denom
}

func CmpFilesChebyshev(a, b *LoadedFile) float64 {
	max := 0.0
	for i := range a.Data {
		diff := math.Abs(a.Data[i] - b.Data[i])
		if diff > max {
			max = diff
		}
	}
	return max
}

func CmpFilesCosine(a, b *LoadedFile) float64 {
	dotProduct := 0.0
	normA := 0.0
	normB := 0.0
	for i := range a.Data {
		dotProduct += a.Data[i] * b.Data[i]
		normA += a.Data[i] * a.Data[i]
		normB += b.Data[i] * b.Data[i]
	}
	return dotProduct / (math.Sqrt(normA) * math.Sqrt(normB))
}

func CmpFilesEuclidean(a, b *LoadedFile) float64 {
	sum := 0.0
	for i := range a.Data {
		diff := a.Data[i] - b.Data[i]
		sum += diff * diff
	}
	return math.Sqrt(sum)
}

func CmpFilesJaccard(a, b *LoadedFile) float64 {
	setA := make(map[float64]bool)
	setB := make(map[float64]bool)

	for _, v := range a.Data {
		setA[v] = true
	}

	for _, v := range b.Data {
		setB[v] = true
	}

	intersection := 0
	union := len(setA)

	for k := range setB {
		if setA[k] {
			intersection++
		} else {
			union++
		}
	}

	if union == 0 {
		return 1.0
	}

	return float64(intersection) / float64(union)
}

func CmpFilesManhattan(a, b *LoadedFile) float64 {
	sum := 0.0
	for i := range a.Data {
		sum += math.Abs(a.Data[i] - b.Data[i])
	}
	return sum
}

func CmpFilesHamming(a, b *LoadedFile) float64 {
	count := 0.0
	for i := range a.Data {
		if a.Data[i] != b.Data[i] {
			count++
		}
	}
	return count / float64(len(a.Data))
}

func CmpFilesSpearman(a, b *LoadedFile) float64 {
	rankA := rank(a.Data)
	rankB := rank(b.Data)
	return CmpFilesPearson(&LoadedFile{Data: rankA}, &LoadedFile{Data: rankB})
}

func CmpFilesPearson(a, b *LoadedFile) float64 {
	meanA := mean(a.Data)
	meanB := mean(b.Data)
	num := 0.0
	denomA := 0.0
	denomB := 0.0
	for i := range a.Data {
		diffA := a.Data[i] - meanA
		diffB := b.Data[i] - meanB
		num += diffA * diffB
		denomA += diffA * diffA
		denomB += diffB * diffB
	}
	return num / (math.Sqrt(denomA) * math.Sqrt(denomB))
}

func mean(data []float64) float64 {
	sum := 0.0
	for _, v := range data {
		sum += v
	}
	return sum / float64(len(data))
}

var (
	rankCache = make(map[string][]float64)
	rankMutex sync.Mutex
)

func generateKey(data []float64) string {
	hash := sha256.New()
	for _, v := range data {
		hash.Write([]byte(fmt.Sprintf("%.6f", v)))
	}
	return hex.EncodeToString(hash.Sum(nil))
}

func rank(data []float64) []float64 {
	key := generateKey(data)

	rankMutex.Lock()
	if cachedRank, found := rankCache[key]; found {
		rankMutex.Unlock()
		return cachedRank
	}
	rankMutex.Unlock()

	ranked := make([]float64, len(data))
	for i, v := range data {
		rank := 1
		for j, w := range data {
			if i != j && w < v {
				rank++
			}
		}
		ranked[i] = float64(rank)
	}

	rankMutex.Lock()
	rankCache[key] = ranked
	rankMutex.Unlock()

	return ranked
}

func main() {
	P := flag.Float64(
		"P",
		0.0,
		"Normalization parameter. Applies normalization to data using the specified step size P if P is non-zero.",
	)
	N := flag.Int(
		"N",
		0,
		"Sequence length. Truncates data sequences to length N. If set to 0, uses the length of the first non-missing file's data.",
	)
	method := flag.String(
		"method",
		"all",
		`Comparison method to use. Supported methods include:
			- jaccard
			- euclidean
			- cosine
			- pearson
			- manhattan
			- chebyshev
			- canberra
			- braycurtis
			- minkowski
			- hamming
			- chiSquare
			- klDivergence
			- bhattacharyya
			- mahalanobis
			- spearman
			- kendall
			- tanimoto
			- overlap
			- hausdorff
			- dynamicTimeWarping
			- editDistance
			- tversky
			- all (to execute all methods)`,
	)
	inputFile := flag.String(
		"input",
		"",
		"Path to the input JSON file containing an array of LoadedFile objects to be compared.",
	)
	outputFile := flag.String(
		"output",
		"",
		"Path to the output CSV file where the comparison matrix will be saved.",
	)

	flag.Usage = func() {
		fmt.Fprintf(flag.CommandLine.Output(), "Usage of %s:\n", os.Args[0])
		fmt.Println("\nRequired Flags:")
		fmt.Println("  -input string")
		fmt.Println("        Path to the input JSON file containing an array of LoadedFile objects to be compared.")
		fmt.Println("  -output string")
		fmt.Println("        Path to the output CSV file where the comparison matrix will be saved.")
		fmt.Println("\nOptional Flags:")
		fmt.Println("  -P float")
		fmt.Println("        Normalization parameter. Applies normalization to data using the specified step size P if P is non-zero. (default 0.0)")
		fmt.Println("  -N int")
		fmt.Println("        Sequence length. Truncates or pads data sequences to length N. If set to 0, uses the length of the first non-missing file's data. (default 0)")
		fmt.Println("  -method string")
		fmt.Println("        Comparison method to use. Supported methods include:")
		fmt.Println(`          jaccard, euclidean, cosine, pearson, manhattan, chebyshev, canberra, braycurtis, minkowski, hamming,
				chiSquare, klDivergence, bhattacharyya, mahalanobis, spearman, kendall, tanimoto, overlap, hausdorff,
				dynamicTimeWarping, editDistance, tversky, all (to execute all methods). (default "all")`)
		fmt.Println("\nExample:")
		fmt.Printf("  %s -P=0.1 -N=100 -B=10 -method=euclidean -input=data.json -output=results.csv\n", os.Args[0])
	}

	flag.Parse()

	if *inputFile == "" {
		fmt.Println("Error: -input flag is required.")
		flag.Usage()
		os.Exit(1)
	}

	if *outputFile == "" {
		fmt.Println("Error: -output flag is required.")
		flag.Usage()
		os.Exit(1)
	}

	fmt.Println("Configuration:")
	fmt.Printf("  Normalization Parameter (P): %f\n", *P)
	fmt.Printf("  Sequence Length (N): %d\n", *N)
	fmt.Printf("  Comparison Method: %s\n", *method)
	fmt.Printf("  Input File: %s\n", *inputFile)
	fmt.Printf("  Output File: %s\n", *outputFile)

	content, err := ioutil.ReadFile(*inputFile)
	if err != nil {
		panic(err)
	}

	var files []LoadedFile
	err = json.Unmarshal(content, &files)
	if err != nil {
		panic(err)
	}

	// Clear content so it can be GC'd
	content = nil

	// Reduce the data length to N for each file
	if *N == 0 {
		// Find the first non-missing file
		for _, file := range files {
			if !file.Missing {
				fmt.Printf("Using %s as the length of the sequences (N=%d)\n", file.Path, len(file.Data))
				*N = len(file.Data)
				break
			}
		}
	}

	for i := range files {
		if !files[i].Missing {
			if len(files[i].Data) < *N {
				fmt.Printf("Warning: file %s has less than %d elements\n", files[i].Path, *N)
			}

			files[i].Data = files[i].Data[:*N]
		}
	}

	// Apply norm if P is not 0
	if *P != 0 {
		for i := range files {
			applyNorm(files[i].Data, *P)
		}
	}

	// Sort the files by name
	sort.Slice(files, func(i, j int) bool {
		return files[i].Path < files[j].Path
	})

	// If method is empty or all, compare all methods
	if *method == "" || *method == "all" {
		methods := []string{
			"jaccard",
			"euclidean",
			"cosine",
			"pearson",
			"manhattan",
			"chebyshev",
			"canberra",
			"braycurtis",
			"minkowski",
			"hamming",
			"chiSquare",
			"klDivergence",
			"bhattacharyya",
			"mahalanobis",
			"spearman",
			"kendall",
			"tanimoto",
			"overlap",
			"hausdorff",
			"dynamicTimeWarping",
			"editDistance",
			"tversky",
		}

		for _, smethod := range methods {
			fmt.Printf("Submethod: %s\n", smethod)
			generateCmpMatrix(files, &smethod, outputFile, P, N)
		}
	} else {
		generateCmpMatrix(files, method, outputFile, P, N)
	}
}

func populateRankCache(files []LoadedFile) {
	numCPU := runtime.NumCPU()
	n := len(files)
	totalRanks := int64(n)
	completedRanks := int64(0)

	startTime := time.Now()

	// Progress bar update goroutine
	go func() {
		for {
			completed := atomic.LoadInt64(&completedRanks)
			progress := float64(completed) / float64(totalRanks) * 100

			elapsedTime := time.Since(startTime)
			timePerRank := elapsedTime.Seconds() / float64(completed)
			remainingRanks := totalRanks - completed
			estimatedRemainingTime := time.Duration(float64(remainingRanks) * timePerRank * float64(time.Second))

			fmt.Printf("\rComputing Ranks: [%-50s] %.2f%% | ETA: %v",
				strings.Repeat("=", int(progress/2)),
				progress,
				formatDuration(estimatedRemainingTime))

			if progress >= 100 {
				fmt.Println()
				return
			}
			time.Sleep(500 * time.Millisecond)
		}
	}()

	// Worker function
	worker := func(start, end int) {
		for i := start; i < end; i++ {
			rank(files[i].Data)
			atomic.AddInt64(&completedRanks, 1)
		}
	}

	// Start worker goroutines
	chunkSize := n / numCPU
	for w := 0; w < numCPU; w++ {
		start := w * chunkSize
		end := start + chunkSize
		if w == numCPU-1 {
			end = n
		}
		go worker(start, end)
	}

	// Wait for all rank computations to complete
	for atomic.LoadInt64(&completedRanks) < totalRanks {
		time.Sleep(100 * time.Millisecond)
	}

	fmt.Println("\nAll rank computations completed")
}

func generateCmpMatrix(files []LoadedFile, method *string, outputFile *string, P *float64, N *int) {
	// If method uses ranks, populate the rank cache
	if *method == "spearman" || *method == "pearson" {
		populateRankCache(files)
	}

	// Create a matrix of distances
	matrix := make([][]float64, len(files))
	for i := range matrix {
		matrix[i] = make([]float64, len(files))
	}

	numCPU := runtime.NumCPU()
	n := len(files)
	totalComparisons := int64((n * (n + 1)) / 2)
	completedComparisons := int64(0)

	startTime := time.Now()

	// Progress bar update goroutine
	go func() {
		for {
			completed := atomic.LoadInt64(&completedComparisons)
			progress := float64(completed) / float64(totalComparisons) * 100

			elapsedTime := time.Since(startTime)
			timePerComparison := elapsedTime.Seconds() / float64(completed)
			remainingComparisons := totalComparisons - completed
			estimatedRemainingTime := time.Duration(float64(remainingComparisons) * timePerComparison * float64(time.Second))

			fmt.Printf("\rProgress: [%-50s] %.2f%% | ETA: %v",
				strings.Repeat("=", int(progress/2)),
				progress,
				formatDuration(estimatedRemainingTime))

			if progress >= 100 {
				fmt.Println()
				return
			}
			time.Sleep(500 * time.Millisecond)
		}
	}()

	// Worker function
	worker := func(start, end int) {
		for i := start; i < end; i++ {
			for j := i; j < n; j++ {
				fi, fj := &files[i], &files[j]
				var result float64
				if fi.Missing || fj.Missing {
					result = math.NaN()
				} else {
					result = CmpFiles(fi, fj, *method)
				}
				matrix[i][j] = result
				if i != j {
					matrix[j][i] = result
				}
				atomic.AddInt64(&completedComparisons, 1)
			}
		}
	}

	// Start worker goroutines
	chunkSize := n / numCPU
	for w := 0; w < numCPU; w++ {
		start := w * chunkSize
		end := start + chunkSize
		if w == numCPU-1 {
			end = n
		}
		go worker(start, end)
	}

	// Wait for all comparisons to complete
	for atomic.LoadInt64(&completedComparisons) < totalComparisons {
		time.Sleep(100 * time.Millisecond)
	}

	fmt.Println("\nAll comparisons completed")

	// Save the matrix as a CSV file
	outputFileName := fmt.Sprintf("%s_%s_%f_%d.csv", *outputFile, *method, *P, *N)
	fmt.Printf("Saving results to %s\n", outputFileName)

	file, err := os.Create(outputFileName)
	if err != nil {
		panic(err)
	}

	defer file.Close()

	// Print headers on the first row and column
	file.WriteString("File")
	for i := 0; i < len(files); i++ {
		file.WriteString(",")
		file.WriteString(files[i].Path)
	}

	file.WriteString("\n")

	// Print the matrix
	for i := 0; i < len(files); i++ {
		file.WriteString(files[i].Path)
		for j := 0; j < len(files); j++ {
			file.WriteString(fmt.Sprintf(",%f", matrix[i][j]))
		}
		file.WriteString("\n")
	}
}

func formatDuration(d time.Duration) string {
	d = d.Round(time.Second)
	h := d / time.Hour
	d -= h * time.Hour
	m := d / time.Minute
	d -= m * time.Minute
	s := d / time.Second
	return fmt.Sprintf("%02d:%02d:%02d", h, m, s)
}
