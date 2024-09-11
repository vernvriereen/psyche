pub fn make_pretraining_samples(tokens: &[i32]) -> (&[i32], &[i32]) {
    (&tokens[..tokens.len() - 1], &tokens[1..])
}

// TODO: make_finetuning_samples
