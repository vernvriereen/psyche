use anyhow::{bail, Result};
use std::{collections::HashSet, path::PathBuf};
use tch::{nn::VarStore, Device, Kind, Tensor};

pub fn load_safetensors_into_variables(vs: &mut VarStore, repo_files: &[PathBuf]) -> Result<()> {
    let mut unmatched = vs
        .variables()
        .keys()
        .map(|x| x.clone())
        .collect::<HashSet<_>>();
    for path in repo_files.iter().filter(|x| {
        x.extension()
            .is_some_and(|y| y.eq_ignore_ascii_case("safetensors"))
    }) {
        let file = std::fs::File::open(path)?;
        let content = unsafe { memmap2::MmapOptions::new().map(&file)? };
        let safetensors = safetensors::SafeTensors::deserialize(&content)?;
        let mut variables = vs.variables_.lock().unwrap();
        for (name, var) in variables.named_variables.iter_mut() {
            if let Ok(view) = safetensors.tensor(name) {
                let size: Vec<i64> = view.shape().iter().map(|&x| x as i64).collect();
                let kind: Kind = view.dtype().try_into()?;
                let src_tensor = unsafe {
                    Tensor::from_blob(view.data().as_ptr(), &size, &[], kind, Device::Cpu)
                };
                var.f_copy_(&src_tensor)?;
                unmatched.remove(name);
            }
        }
    }
    if !unmatched.is_empty() {
        bail!(
            "Checkpoint missing the following variables: {:?}",
            unmatched
        );
    }
    Ok(())
}
