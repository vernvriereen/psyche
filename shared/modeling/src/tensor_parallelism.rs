use anyhow::{bail, Result};
use std::{collections::HashMap, sync::Arc};
use tch::{
    nn::{self, Module, Shard, VarStore},
    Device, Tensor,
};

#[cfg(feature = "parallelism")]
use tch::{CStore, ReduceOpType, CNCCL};

#[cfg(feature = "parallelism")]
pub type Communicator = CNCCL;

#[cfg(feature = "parallelism")]
pub type CommunicatorId = CStore;

#[cfg(not(feature = "parallelism"))]
#[derive(Debug)]
pub struct Communicator;

#[cfg(not(feature = "parallelism"))]
#[derive(Debug, Copy, Clone)]
pub struct CommunicatorId;

#[cfg(not(feature = "parallelism"))]
impl Communicator {
    pub fn size(&self) -> i64 {
        unimplemented!()
    }

    pub fn rank(&self) -> usize {
        unimplemented!()
    }
}

#[cfg(not(feature = "parallelism"))]
impl Default for CommunicatorId {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(feature = "parallelism"))]
impl CommunicatorId {
    pub fn new() -> Self {
        unimplemented!()
    }
}

pub enum ReduceType {
    Sum,
    Max,
}

#[cfg(feature = "parallelism")]
impl From<ReduceType> for ReduceOpType {
    fn from(value: ReduceType) -> Self {
        match value {
            ReduceType::Sum => ReduceOpType::Sum,
            ReduceType::Max => ReduceOpType::Max,
        }
    }
}

pub trait AllReduce {
    fn all_reduce_(&mut self, comm: &Option<Arc<Communicator>>, op: ReduceType);
}

pub trait CudaSynchronize {
    fn cuda_synchronize(&self);
}

impl AllReduce for Tensor {
    #[cfg(feature = "parallelism")]
    fn all_reduce_(&mut self, comm: &Option<Arc<Communicator>>, op: ReduceType) {
        if let Some(comm) = comm {
            let device = self.device();
            comm.all_reduce(&[self], op.into()).unwrap();
            device.cuda_synchronize();
        }
    }

    #[cfg(not(feature = "parallelism"))]
    fn all_reduce_(&mut self, comm: &Option<Arc<Communicator>>, _op: ReduceType) {
        assert!(comm.is_none());
    }
}

impl CudaSynchronize for Device {
    fn cuda_synchronize(&self) {
        match &self {
            Device::Cuda(rank) => tch::Cuda::synchronize(*rank as i64),
            _ => panic!("Cannot CUDA synchronize non-CUDA device"),
        }
    }
}

pub trait ModelParallelRegion {
    fn copy_to_model_parallel_region(&self, comm: &Option<Arc<Communicator>>) -> Tensor;
    fn reduce_from_model_parallel_region(&self, comm: &Option<Arc<Communicator>>) -> Tensor;
    fn scatter_to_model_parallel_region(&self, comm: &Option<Arc<Communicator>>) -> Tensor;
    fn gather_from_model_parallel_region(&self, comm: &Option<Arc<Communicator>>) -> Tensor;
}

impl ModelParallelRegion for Tensor {
    #[cfg(feature = "parallelism")]
    fn copy_to_model_parallel_region(&self, comm: &Option<Arc<Communicator>>) -> Tensor {
        match comm {
            Some(comm) => comm.copy_to_model_parallel(self).unwrap(),
            None => self.shallow_clone(),
        }
    }

    #[cfg(feature = "parallelism")]
    fn reduce_from_model_parallel_region(&self, comm: &Option<Arc<Communicator>>) -> Tensor {
        match comm {
            Some(comm) => comm.reduce_from_model_parallel(self).unwrap(),
            None => self.shallow_clone(),
        }
    }

    #[cfg(feature = "parallelism")]
    fn scatter_to_model_parallel_region(&self, comm: &Option<Arc<Communicator>>) -> Tensor {
        match comm {
            Some(comm) => comm.scatter_to_model_parallel(self).unwrap(),
            None => self.shallow_clone(),
        }
    }

    #[cfg(feature = "parallelism")]
    fn gather_from_model_parallel_region(&self, comm: &Option<Arc<Communicator>>) -> Tensor {
        match comm {
            Some(comm) => comm.gather_from_model_parallel(self).unwrap(),
            None => self.shallow_clone(),
        }
    }

    #[cfg(not(feature = "parallelism"))]
    fn copy_to_model_parallel_region(&self, comm: &Option<Arc<Communicator>>) -> Tensor {
        assert!(comm.is_none());
        self.shallow_clone()
    }

    #[cfg(not(feature = "parallelism"))]
    fn reduce_from_model_parallel_region(&self, comm: &Option<Arc<Communicator>>) -> Tensor {
        assert!(comm.is_none());
        self.shallow_clone()
    }

    #[cfg(not(feature = "parallelism"))]
    fn scatter_to_model_parallel_region(&self, comm: &Option<Arc<Communicator>>) -> Tensor {
        assert!(comm.is_none());
        self.shallow_clone()
    }

    #[cfg(not(feature = "parallelism"))]
    fn gather_from_model_parallel_region(&self, comm: &Option<Arc<Communicator>>) -> Tensor {
        assert!(comm.is_none());
        self.shallow_clone()
    }
}

#[derive(Debug)]
pub struct ColumnParallelLinear {
    linear: nn::Linear,
    comm: Option<Arc<Communicator>>,
    gather_output: bool,
}

#[derive(Debug)]
pub struct RowParallelLinear {
    linear: nn::Linear,
    comm: Option<Arc<Communicator>>,
    input_is_parallel: bool,
}

impl ColumnParallelLinear {
    pub fn new(
        vs: nn::Path,
        in_features: i64,
        out_features: i64,
        bias: bool,
        gather_output: bool,
        comm: Option<Arc<Communicator>>,
    ) -> Self {
        let world_size = comm.as_ref().map(|c| c.size()).unwrap_or(1);
        assert_eq!(
            out_features % world_size,
            0,
            "out_features must be divisible by world_size"
        );

        let linear = nn::linear(
            &vs,
            in_features,
            out_features,
            nn::LinearConfig {
                bias,
                shard: comm.as_ref().map(|comm| Shard {
                    dim: 0,
                    rank: comm.rank() as usize,
                    world_size: comm.size() as usize,
                }),
                ..Default::default()
            },
        );

        Self {
            linear,
            comm,
            gather_output,
        }
    }
}

impl Module for ColumnParallelLinear {
    fn forward(&self, input: &Tensor) -> Tensor {
        match &self.comm {
            Some(_) => {
                let device = input.device();
                let input_parallel = input.copy_to_model_parallel_region(&self.comm).contiguous();
                let output_parallel = self.linear.forward(&input_parallel);

                let ret = if self.gather_output {
                    output_parallel.gather_from_model_parallel_region(&self.comm)
                } else {
                    output_parallel
                };
                device.cuda_synchronize();
                ret
            }
            None => self.linear.forward(input),
        }
    }
}

unsafe impl Send for ColumnParallelLinear {}

impl RowParallelLinear {
    pub fn new(
        vs: nn::Path,
        in_features: i64,
        out_features: i64,
        bias: bool,
        input_is_parallel: bool,
        comm: Option<Arc<Communicator>>,
    ) -> Self {
        let world_size = comm.as_ref().map(|c| c.size()).unwrap_or(1);
        assert_eq!(
            in_features % world_size,
            0,
            "in_features must be divisible by world_size"
        );

        let linear = nn::linear(
            &vs,
            in_features,
            out_features,
            nn::LinearConfig {
                bias,
                shard: comm.as_ref().map(|comm| Shard {
                    dim: 1,
                    rank: comm.rank() as usize,
                    world_size: comm.size() as usize,
                }),
                ..Default::default()
            },
        );

        Self {
            linear,
            comm,
            input_is_parallel,
        }
    }
}

impl Module for RowParallelLinear {
    fn forward(&self, input: &Tensor) -> Tensor {
        match &self.comm {
            Some(_) => {
                let device = input.device();

                let input_parallel = if self.input_is_parallel {
                    input.shallow_clone()
                } else {
                    input.scatter_to_model_parallel_region(&self.comm)
                };

                let output_parallel = self.linear.forward(&input_parallel);
                let ret = output_parallel.reduce_from_model_parallel_region(&self.comm);
                device.cuda_synchronize();
                ret
            }
            None => self.linear.forward(input),
        }
    }
}

unsafe impl Send for RowParallelLinear {}

#[allow(unused)]
pub fn unshard_tensor(sharded_tensors: Vec<Tensor>, shard: &Shard) -> Tensor {
    let Shard {
        dim, world_size, ..
    } = *shard;

    let mut full_shape = sharded_tensors[0].size();
    let shard_size = full_shape[dim];
    full_shape[dim] = shard_size * (world_size as i64);

    let full_tensor = Tensor::empty(
        &full_shape,
        (sharded_tensors[0].kind(), sharded_tensors[0].device()),
    );

    for (rank, shard_tensor) in sharded_tensors.into_iter().enumerate() {
        let start = (rank as i64) * shard_size;
        let end = ((rank + 1) as i64) * shard_size;

        let mut slice = full_tensor.slice(dim as i64, start, Some(end), 1);
        slice.copy_(&shard_tensor);
    }

    full_tensor
}

#[allow(unused)]
pub fn tensor_shard(full_tensor: &Tensor, shard: &Shard) -> Tensor {
    let Shard {
        dim,
        world_size,
        rank,
    } = *shard;

    let full_shape = full_tensor.size();
    let total_size = full_shape[dim];

    let shard_size = total_size / (world_size as i64);
    let start = (rank as i64) * shard_size;
    let end = ((rank + 1) as i64) * shard_size;

    full_tensor.slice(dim as i64, start, Some(end), 1)
}

#[allow(unused)]
pub fn unsharded_tensor_size(reference_shape: &[i64], shard: &Shard) -> Vec<i64> {
    let Shard {
        dim, world_size, ..
    } = *shard;

    let shard_size = reference_shape[dim];
    let total_size = shard_size * (world_size as i64);

    let mut unsharded_shape = reference_shape.to_vec();
    unsharded_shape[dim] = total_size;

    unsharded_shape
}

// we only actually build the model on rank 0, all other ranks return an empty map (but perform tp)
pub fn unsharded_cpu_variables(
    vs: &VarStore,
    comm: Option<Arc<Communicator>>,
) -> Result<HashMap<String, Tensor>> {
    let _no_grad = tch::no_grad_guard();
    let mut ret = match comm.as_ref().map(|x| x.rank() == 0).unwrap_or(true) {
        true => Some(HashMap::new()),
        false => None,
    };
    let variables = vs.variables_.lock().unwrap();
    let shards = variables.shards.clone();
    let mut variables = variables.named_variables.iter().collect::<Vec<_>>();
    variables.sort_by_key(|x| x.0);
    for (name, var) in variables {
        let var = match shards.get(name) {
            #[cfg(feature = "parallelism")]
            Some(shard) => {
                let shards = (0..shard.world_size)
                    .map(|_| var.empty_like())
                    .collect::<Vec<_>>();
                match &comm {
                    Some(comm) => comm.all_gather(&shards, var)?,
                    None => {
                        bail!("Found sharded tensor {} but no communicator", name);
                    }
                };
                unshard_tensor(shards, shard)
            }
            #[cfg(not(feature = "parallelism"))]
            Some(_) => bail!("Sharded model but parallelism feature turned off"),
            None => var.shallow_clone(),
        };
        // now you're probably thinking, why are you moving this to the CPU? why even unshard the tensor
        // on the other ranks and not do it just on rank 0? here's the thing, you're right, you're absoutely right,
        // except horribly, inexplicibly wrong. if you do that, the non-zero ranks fill up and can OOM -- the gathered
        // tensors hang around for no reason. doing this operation on all ranks makes the memory free as one would expect.
        // remember, we're just along for the ride.
        let var = var.to_device(Device::Cpu);
        if let Some(ret) = ret.as_mut() {
            ret.insert(name.to_owned(), var);
        }
    }
    Ok(ret.unwrap_or_default())
}
