use anyhow::{bail, Result};
use safetensors::slice::TensorIndexer;
use std::{collections::HashSet, ops::Bound, path::PathBuf};
use tch::{nn::{Shard, VarStore}, Device, Kind, Tensor};

pub fn load_safetensors_into_variables(vs: &mut VarStore, repo_files: &[PathBuf]) -> Result<()> {
    let mut unmatched = vs.variables().keys().cloned().collect::<HashSet<_>>();
    for path in repo_files.iter().filter(|x| {
        x.extension()
            .is_some_and(|y| y.eq_ignore_ascii_case("safetensors"))
    }) {
        let file = std::fs::File::open(path)?;
        let content = unsafe { memmap2::MmapOptions::new().map(&file)? };
        let safetensors = safetensors::SafeTensors::deserialize(&content)?;
        let mut variables = vs.variables_.lock().unwrap();
        let shards = variables.shards.clone();
        for (name, var) in variables.named_variables.iter_mut() {
            if let Ok(view) = safetensors.tensor(name) {
                let mut size: Vec<i64> = view.shape().iter().map(|&x| x as i64).collect();
                let kind: Kind = view.dtype().try_into()?;

                if let Some(Shard { dim, rank, world_size }) = shards.get(name) {
                    let (dim, rank, world_size) = (*dim, *rank, *world_size);
                    let total_size = size[dim];
                    if total_size % (world_size as i64) != 0 {
                        bail!(
                            "Cannot shard tensor {} of shape {:?} along dimension {} into {} parts",
                            name,
                            size,
                            dim,
                            world_size
                        );
                    }
                    let block_size = total_size / (world_size as i64);
                    let start = (rank as i64) * block_size;
                    let stop = ((rank + 1) as i64) * block_size;

                    let slices: Vec<TensorIndexer> = (0..view.shape().len())
                        .map(|i| {
                            if i == dim {
                                TensorIndexer::Narrow(
                                    Bound::Included(start as usize),
                                    Bound::Excluded(stop as usize),
                                )
                            } else {
                                TensorIndexer::Narrow(Bound::Unbounded, Bound::Unbounded)
                            }
                        })
                        .collect();
                    let data_iterator = match view.sliced_data(&slices) {
                        Ok(data_iterator) => data_iterator,
                        Err(_) => {
                            bail!("safetenors slice error");
                        }
                    };
                    let data: Vec<u8> = data_iterator.flatten().cloned().collect();
                    size[dim] = block_size;
                    let src_tensor = unsafe {
                        Tensor::from_blob(data.as_ptr(), &size, &[], kind, Device::Cpu)
                    };
                    var.f_copy_(&src_tensor)?;
                } else {
                    let src_tensor = unsafe {
                        Tensor::from_blob(view.data().as_ptr(), &size, &[], kind, Device::Cpu)
                    };
                    var.f_copy_(&src_tensor)?;
                }
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
