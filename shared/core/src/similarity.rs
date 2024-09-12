
pub struct DistanceThresholds {
    pub jaccard_threshold: f32,
    pub manhattan_threshold: f32,
    pub hamming_threshold: f32,
}

pub fn is_similar(a: &[f32], b: &[f32], thresholds: &DistanceThresholds) -> Result<bool, &'static str> {
    let manhattan = manhattan_distance(a, b)?;
    if manhattan > thresholds.manhattan_threshold {
        return Ok(false);
    }

    let hamming = hamming_distance(a, b)?;
    if hamming > thresholds.hamming_threshold {
        return Ok(false);
    }

    let jaccard = jaccard_distance(a, b);
    if jaccard > thresholds.jaccard_threshold {
        return Ok(false);
    }

    Ok(true)
}

pub fn jaccard_distance(a: &[f32], b: &[f32]) -> f32 {
    let mut intersection = 0;
    let mut union = a.len();

    for &val in b {
        if a.contains(&val) {
            intersection += 1;
        } else {
            union += 1;
        }
    }

    if union == 0 {
        return 0.0;
    }

    1.0 - (intersection as f32 / union as f32)
}

pub fn manhattan_distance(a: &[f32], b: &[f32]) -> Result<f32, &'static str> {
    if a.len() != b.len() {
        return Err("Input arrays must have the same length");
    }

    if a.is_empty() {
        return Err("Input arrays must not be empty");
    }

    Ok(a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .sum())
}

pub fn hamming_distance(a: &[f32], b: &[f32]) -> Result<f32, &'static str> {
    if a.len() != b.len() {
        return Err("Input arrays must have the same length");
    }

    if a.is_empty() {
        return Err("Input arrays must not be empty");
    }

    let count: f32 = a.iter()
        .zip(b.iter())
        .filter(|(x, y)| x != y)
        .count() as f32;

    Ok(count / a.len() as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_jaccard_tests(a: &[f32], b: &[f32], expected: f32) {
        // Print the result
        println!("Jaccard: {}", jaccard_distance(a, b));
        assert!((jaccard_distance(a, b) - expected).abs() < 1e-6);
    }

    fn run_manhattan_tests(a: &[f32], b: &[f32], expected: Result<f32, &'static str>) {
        match manhattan_distance(a, b) {
            Ok(result) => assert!((result - expected.unwrap()).abs() < 1e-6),
            Err(e) => assert_eq!(Err(e), expected),
        }
    }

    fn run_hamming_tests(a: &[f32], b: &[f32], expected: Result<f32, &'static str>) {
        match hamming_distance(a, b) {
            Ok(result) => assert!((result - expected.unwrap()).abs() < 1e-6),
            Err(e) => assert_eq!(Err(e), expected),
        }
    }

    #[test]
    fn test_zero_length_inputs() {
        let a: [f32; 0] = [];
        let b: [f32; 0] = [];

        // Jaccard should return 0.0 for empty inputs as per the implementation
        run_jaccard_tests(&a, &b, 0.0);

        // Hamming and manhattan should return errors
        run_manhattan_tests(&a, &b, Err("Input arrays must not be empty"));
        run_hamming_tests(&a, &b, Err("Input arrays must not be empty"));
    }

    #[test]
    fn test_mismatched_lengths() {
        let a = [1.0, 2.0, 3.0];
        let b = [1.0, 2.0];

        // Jaccard can handle different lengths because it's set-based
        run_jaccard_tests(&a, &b, 0.3333333333333333);

        // Manhattan and Hamming should return errors for mismatched lengths
        run_manhattan_tests(&a, &b, Err("Input arrays must have the same length"));
        run_hamming_tests(&a, &b, Err("Input arrays must have the same length"));
    }

    #[test]
    fn test_no_intersection() {
        let a = [1.0, 2.0, 3.0];
        let b = [4.0, 5.0, 6.0];
        run_jaccard_tests(&a, &b, 1.0);
        run_manhattan_tests(&a, &b, Ok(9.0));
        run_hamming_tests(&a, &b, Ok(1.0));
    }

    #[test]
    fn test_partial_intersection() {
        let a = [1.0, 2.0, 5.0];
        let b = [1.0, 2.0, 3.0];
        run_jaccard_tests(&a, &b, 0.5);
        run_manhattan_tests(&a, &b, Ok(2.0));
        run_hamming_tests(&a, &b, Ok(0.333333));
    }

    #[test]
    fn test_complete_overlap() {
        let a = [1.0, 2.0, 3.0];
        let b = [1.0, 2.0, 3.0];
        run_jaccard_tests(&a, &b, 0.0);
        run_manhattan_tests(&a, &b, Ok(0.0));
        run_hamming_tests(&a, &b, Ok(0.0));
    }

    #[test]
    fn test_partial_match_floats() {
        let a = [0.1, 0.2, 0.3];
        let b = [0.1, 0.3, 1.0];
        run_jaccard_tests(&a, &b, 0.5);
        run_manhattan_tests(&a, &b, Ok(0.8));
        run_hamming_tests(&[0.0, 0.0, 0.0], &[0.0, 0.0, 1.0], Ok(0.333333));
    }

    #[test]
    fn test_no_match_floats() {
        let a = [0.1, 0.2, 0.3];
        let b = [0.4, 0.5, 9.1];
        run_jaccard_tests(&a, &b, 1.0);
        run_manhattan_tests(&a, &b, Ok(9.4));
        run_hamming_tests(&[0.0, 0.0, 0.0], &[0.0, 0.0, 9.0], Ok(0.333333));
    }

    #[test]
    fn test_some_intersection() {
        let a = [1.5, 2.5, 3.5, 4.5];
        let b = [2.5, 3.5, 5.5, 1.1];
        run_jaccard_tests(&a, &b, 0.6666666);
        run_manhattan_tests(&a, &b, Ok(7.4));
        run_hamming_tests(&[1.0, 2.0, 3.0, 4.0], &[2.0, 3.0, 5.0, 1.0], Ok(1.0));
    }

    #[test]
    fn test_partial_overlap_large_numbers() {
        let a = [100.0, 200.0, 300.0, 1000.0];
        let b = [100.0, 150.0, 250.0, 300.0];
        run_jaccard_tests(&a, &b, 0.6666666);
        run_manhattan_tests(&a, &b, Ok(800.0));
        run_hamming_tests(&[100.0, 200.0, 300.0, 1000.0], &[100.0, 150.0, 250.0, 300.0], Ok(0.75));
    }

    #[test]
    fn test_is_similar_complete_match() {
        let a = [1.0, 2.0, 3.0];
        let b = [1.0, 2.0, 3.0];
        let thresholds = DistanceThresholds {
            jaccard_threshold: 0.1,
            manhattan_threshold: 1.0,
            hamming_threshold: 0.1,
        };
        
        assert_eq!(is_similar(&a, &b, &thresholds), Ok(true));
    }

    #[test]
    fn test_is_similar_partial_match() {
        let a = [1.0, 2.0, 3.0];
        let b = [1.0, 2.0, 5.0];
        let thresholds = DistanceThresholds {
            jaccard_threshold: 0.6,
            manhattan_threshold: 3.0,
            hamming_threshold: 0.5,
        };
        
        assert_eq!(is_similar(&a, &b, &thresholds), Ok(true));
    }

    #[test]
    fn test_is_similar_exceeds_manhattan() {
        let a = [1.0, 2.0, 3.0];
        let b = [1.0, 5.0, 7.0];
        let thresholds = DistanceThresholds {
            jaccard_threshold: 0.8,
            manhattan_threshold: 2.0,
            hamming_threshold: 0.8,
        };
        
        assert_eq!(is_similar(&a, &b, &thresholds), Ok(false));
    }

    #[test]
    fn test_is_similar_exceeds_hamming() {
        let a = [1.0, 2.0, 3.0];
        let b = [4.0, 5.0, 6.0];
        let thresholds = DistanceThresholds {
            jaccard_threshold: 1.0,
            manhattan_threshold: 10.0,
            hamming_threshold: 0.3,
        };
        
        assert_eq!(is_similar(&a, &b, &thresholds), Ok(false));
    }

    #[test]
    fn test_is_similar_exceeds_jaccard() {
        let a = [1.0, 2.0, 3.0];
        let b = [4.0, 5.0, 6.0];
        let thresholds = DistanceThresholds {
            jaccard_threshold: 0.5,
            manhattan_threshold: 10.0,
            hamming_threshold: 1.0,
        };
        
        assert_eq!(is_similar(&a, &b, &thresholds), Ok(false));
    }
}
