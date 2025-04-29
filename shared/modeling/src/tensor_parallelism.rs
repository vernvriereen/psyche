use anyhow::{bail, Result};
use std::{collections::HashMap, sync::Arc};
use tch::{
    nn::{self, Module, Shard, VarStore}, Device, IndexOp, Kind, Tensor
};
use torch_sys::IntList;

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

    pub fn rank(&self) -> i64 {
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
    Avg,
}

#[cfg(feature = "parallelism")]
impl From<ReduceType> for ReduceOpType {
    fn from(value: ReduceType) -> Self {
        match value {
            ReduceType::Sum => ReduceOpType::Sum,
            ReduceType::Max => ReduceOpType::Max,
            ReduceType::Avg => ReduceOpType::Avg,
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
            comm.all_reduce(&[self], op.into()).unwrap();
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

pub trait ParallelExpandHeads {
    fn parallel_expand_heads(
        &self,
        comm: &Option<Arc<Communicator>>,
        shape: impl IntList,
    ) -> Tensor;
}

fn _expand_heads(tensor: &Tensor, shape: impl IntList) -> Tensor {
    tensor.expand(shape, false)
}

impl ParallelExpandHeads for Tensor {
    #[cfg(feature = "parallelism")]
    fn parallel_expand_heads(
        &self,
        comm: &Option<Arc<Communicator>>,
        shape: impl IntList,
    ) -> Tensor {
        match comm {
            Some(comm) => comm
                .parallel_expand_heads(&self, comm.size() as i64, comm.rank() as i64, shape)
                .unwrap(),
            None => _expand_heads(&self, shape),
        }
    }

    #[cfg(not(feature = "parallelism"))]
    fn parallel_expand_heads(
        &self,
        comm: &Option<Arc<Communicator>>,
        shape: impl IntList,
    ) -> Tensor {
        assert!(comm.is_none());
        _expand_heads(&self, shape)
    }
}

#[derive(Debug)]
pub struct ColumnParallelLinear {
    pub(crate) linear: nn::Linear,
    comm: Option<Arc<Communicator>>,
    gather_output: bool,
}

#[derive(Debug)]
pub struct RowParallelLinear {
    pub(crate) linear: nn::Linear,
    comm: Option<Arc<Communicator>>,
    input_is_parallel: bool,
}

#[derive(Debug)]
pub struct RMSNormParallelInput {
    weight: Tensor,
    eps: f64,
    comm: Option<Arc<Communicator>>,
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

        let comm = match world_size {
            1 => None,
            _ => comm,
        };

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
                let input_parallel = input.copy_to_model_parallel_region(&self.comm).contiguous();
                let output_parallel = self.linear.forward(&input_parallel);

                if self.gather_output {
                    output_parallel.gather_from_model_parallel_region(&self.comm)
                } else {
                    output_parallel
                }
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

        let comm = match world_size {
            1 => None,
            _ => comm,
        };

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
                let input_parallel = if self.input_is_parallel {
                    input.shallow_clone()
                } else {
                    input.scatter_to_model_parallel_region(&self.comm)
                };

                let output_parallel = self.linear.forward(&input_parallel);

                output_parallel.reduce_from_model_parallel_region(&self.comm)
            }
            None => self.linear.forward(input),
        }
    }
}

unsafe impl Send for RowParallelLinear {}

impl RMSNormParallelInput {
    pub fn new(vs: nn::Path, size: i64, eps: f64, comm: Option<Arc<Communicator>>) -> Self {
        let world_size = comm.as_ref().map(|c| c.size()).unwrap_or(1);
        assert_eq!(size % world_size, 0, "size must be divisible by world_size");

        let comm = match world_size {
            1 => None,
            _ => comm,
        };

        let weight = vs.var_with_shard(
            "weight",
            &[size],
            nn::Init::Const(1.),
            comm.as_ref().map(|comm| Shard {
                dim: 0,
                rank: comm.rank() as usize,
                world_size: comm.size() as usize,
            }),
        );

        Self { weight, eps, comm }
    }
}

impl Module for RMSNormParallelInput {
    fn forward(&self, xs: &Tensor) -> Tensor {
        let kind = xs.kind();
        let xs = xs.to_kind(Kind::Float);

        let local_variance = xs.pow_tensor_scalar(2).mean_dim(-1, true, Kind::Float);

        let variance = if let Some(comm) = &self.comm {
            let mut variance = local_variance.shallow_clone();
            variance.all_reduce_(&self.comm, ReduceType::Sum);
            variance / comm.size() as f64 // average across ranks
        } else {
            local_variance
        };

        let xs_normed = xs * (variance + self.eps).rsqrt();
        let xs_normed = xs_normed.to_kind(kind);
        &self.weight * xs_normed
    }
}

unsafe impl Send for RMSNormParallelInput {}

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

#[cfg(test)]
#[cfg(feature = "parallelism")]
pub(crate) mod tests {
    use super::*;
    use crate::{set_suggested_env_vars, set_torch_rng_seed};
    use std::sync::{Arc, Barrier, Mutex};
    use tch::{nn::VarStore, Device, Kind, Tensor};

    fn run_parallel_test<F>(world_size: usize, test_fn: F)
    where
        F: Fn(Arc<CommunicatorId>, usize, Arc<Barrier>, Device) -> Result<()>
            + Send
            + Sync
            + 'static,
    {
        if !tch::utils::has_cuda() || tch::Cuda::device_count() < world_size as i64 {
            println!(
                "Skipping parallel test: requires CUDA and {} GPUs.",
                world_size
            );
            return;
        }

        let barrier = Arc::new(Barrier::new(world_size));
        let comm_id = Arc::new(CommunicatorId::new());
        let test_fn = Arc::new(test_fn);

        let threads: Vec<_> = (0..world_size)
            .map(|rank| {
                let barrier = barrier.clone();
                let comm_id = comm_id.clone();
                let test_fn = test_fn.clone();
                let device = Device::Cuda(rank);

                std::thread::spawn(move || {
                    test_fn(comm_id, rank, barrier, device).unwrap();
                })
            })
            .collect();

        for thread in threads {
            thread.join().expect("Thread panicked");
        }
    }

    #[test]
    fn test_column_parallel_linear_backward() -> Result<()> {
        const WORLD_SIZE: usize = 2;
        const BATCH_SIZE: i64 = 4;
        const SEQ_LEN: i64 = 8;
        const IN_FEATURES: i64 = 16;
        const OUT_FEATURES: i64 = 32; // must be divisible by WORLD_SIZE
        const GATHER_OUTPUT: bool = true;

        assert_eq!(
            OUT_FEATURES % WORLD_SIZE as i64,
            0,
            "OUT_FEATURES must be divisible by WORLD_SIZE"
        );

        set_suggested_env_vars();
        set_torch_rng_seed();

        let input_grads = Arc::new(Mutex::new(Vec::new()));
        let weight_grads_shapes = Arc::new(Mutex::new(Vec::new()));

        {
            let input_grads = input_grads.clone();
            let weight_grads_shapes = weight_grads_shapes.clone();
            run_parallel_test(
                WORLD_SIZE,
                move |comm_id, rank, barrier, device| -> Result<()> {
                    let vs = VarStore::new(device);
                    let comm = Arc::new(CNCCL::new(
                        comm_id.clone(),
                        rank as i64,
                        WORLD_SIZE as i64,
                        device,
                    )?);

                    let layer = ColumnParallelLinear::new(
                        vs.root() / "col_parallel",
                        IN_FEATURES,
                        OUT_FEATURES,
                        false, // no bias
                        GATHER_OUTPUT,
                        Some(comm.clone()),
                    );

                    let input =
                        Tensor::randn(&[BATCH_SIZE, SEQ_LEN, IN_FEATURES], (Kind::Float, device))
                            .set_requires_grad(true);

                    let target_shape = if GATHER_OUTPUT {
                        vec![BATCH_SIZE, SEQ_LEN, OUT_FEATURES]
                    } else {
                        vec![BATCH_SIZE, SEQ_LEN, OUT_FEATURES / WORLD_SIZE as i64]
                    };
                    let target = Tensor::randn(&target_shape, (Kind::Float, device));

                    barrier.wait();
                    let output = layer.forward(&input);
                    barrier.wait();

                    let loss = output.mse_loss(&target, tch::Reduction::Mean);

                    barrier.wait();
                    loss.backward();
                    barrier.wait();

                    input_grads
                        .lock()
                        .unwrap()
                        .push(input.grad().shallow_clone());
                    weight_grads_shapes
                        .lock()
                        .unwrap()
                        .push(layer.linear.ws.grad().size());

                    Ok(())
                },
            );
        }

        let input_grads = input_grads.lock().unwrap();
        let weight_grads_shapes = weight_grads_shapes.lock().unwrap();

        assert_eq!(input_grads.len(), WORLD_SIZE);
        assert_eq!(weight_grads_shapes.len(), WORLD_SIZE);

        for i in 1..WORLD_SIZE {
            assert!(
                input_grads[0].to(Device::Cpu).allclose(
                    &input_grads[i].to(Device::Cpu),
                    1e-5,
                    1e-5,
                    false
                ),
                "Input gradients differ between rank 0 and rank {}",
                i
            );
        }

        let expected_weight_grad_shape = vec![OUT_FEATURES / WORLD_SIZE as i64, IN_FEATURES];
        for (rank, shape) in weight_grads_shapes.iter().enumerate() {
            assert_eq!(
                *shape, expected_weight_grad_shape,
                "Weight gradient shape mismatch on rank {}",
                rank
            );
        }

        Ok(())
    }

    #[test]
    fn test_row_parallel_linear_backward() -> Result<()> {
        const WORLD_SIZE: usize = 2;
        const BATCH_SIZE: i64 = 4;
        const SEQ_LEN: i64 = 8;
        const IN_FEATURES: i64 = 16; // must be divisible by WORLD_SIZE
        const OUT_FEATURES: i64 = 32;

        assert_eq!(
            IN_FEATURES % WORLD_SIZE as i64,
            0,
            "IN_FEATURES must be divisible by WORLD_SIZE for RowParallelLinear"
        );

        set_suggested_env_vars();
        set_torch_rng_seed();

        for (input_is_parallel, bias) in
            [(false, false), (false, true), (true, false), (true, true)]
        {
            let original_input_grads = Arc::new(Mutex::new(Vec::new()));
            let weight_grads_shapes = Arc::new(Mutex::new(Vec::new()));
            let bias_grads = Arc::new(Mutex::new(Vec::new()));

            {
                let original_input_grads = original_input_grads.clone();
                let weight_grads_shapes = weight_grads_shapes.clone();
                let bias_grads = bias_grads.clone();

                run_parallel_test(
                    WORLD_SIZE,
                    move |comm_id, rank, barrier, device| -> Result<()> {
                        let vs = VarStore::new(device);
                        let comm = Arc::new(CNCCL::new(
                            comm_id.clone(),
                            rank as i64,
                            WORLD_SIZE as i64,
                            device,
                        )?);

                        let layer = RowParallelLinear::new(
                            vs.root() / "row_parallel",
                            IN_FEATURES,
                            OUT_FEATURES,
                            bias,
                            input_is_parallel,
                            Some(comm.clone()),
                        );

                        let original_input = Tensor::randn(
                            &[BATCH_SIZE, SEQ_LEN, IN_FEATURES],
                            (Kind::Float, device),
                        )
                        .set_requires_grad(!input_is_parallel);

                        let input_to_layer = if input_is_parallel {
                            let shard_meta = Shard {
                                dim: 2,
                                rank,
                                world_size: WORLD_SIZE,
                            };
                            tensor_shard(&original_input.set_requires_grad(true), &shard_meta)
                                .contiguous()
                        } else {
                            original_input.shallow_clone()
                        };

                        let target = Tensor::randn(
                            &[BATCH_SIZE, SEQ_LEN, OUT_FEATURES],
                            (Kind::Float, device),
                        );

                        barrier.wait();
                        let output = layer.forward(&input_to_layer);
                        barrier.wait();

                        assert_eq!(output.size(), target.size(), "Output shape mismatch");

                        let loss = output.mse_loss(&target, tch::Reduction::Mean);

                        barrier.wait();
                        loss.backward();
                        barrier.wait();

                        if !input_is_parallel {
                            original_input_grads
                                .lock()
                                .unwrap()
                                .push(original_input.grad().shallow_clone());
                        }
                        weight_grads_shapes
                            .lock()
                            .unwrap()
                            .push(layer.linear.ws.grad().size());

                        if bias {
                            bias_grads
                                .lock()
                                .unwrap()
                                .push(layer.linear.bs.as_ref().unwrap().grad().shallow_clone());
                        }

                        Ok(())
                    },
                );
            }

            let original_input_grads = original_input_grads.lock().unwrap();
            let weight_grads_shapes = weight_grads_shapes.lock().unwrap();
            let bias_grads = bias_grads.lock().unwrap();

            assert_eq!(weight_grads_shapes.len(), WORLD_SIZE);
            if bias {
                assert_eq!(bias_grads.len(), WORLD_SIZE);
            }
            if !input_is_parallel {
                assert_eq!(original_input_grads.len(), WORLD_SIZE);
            }

            if !input_is_parallel {
                for i in 1..WORLD_SIZE {
                    assert!(
                        original_input_grads[0].to(Device::Cpu).allclose(
                            &original_input_grads[i].to(Device::Cpu),
                            1e-5,
                            1e-5,
                            false
                        ),
                        "RowParallelLinear (input_is_parallel=false): Original input gradients differ between rank 0 and rank {}",
                        i
                    );
                }
            } else {
            }

            let expected_weight_grad_shape = vec![OUT_FEATURES, IN_FEATURES / WORLD_SIZE as i64];
            for (rank, shape) in weight_grads_shapes.iter().enumerate() {
                assert_eq!(
                    *shape, expected_weight_grad_shape,
                    "RowParallelLinear: Weight gradient shape mismatch on rank {}",
                    rank
                );
            }

            if bias {
                for i in 1..WORLD_SIZE {
                    assert!(
                        bias_grads[0].to(Device::Cpu).allclose(
                            &bias_grads[i].to(Device::Cpu),
                            1e-5,
                            1e-5,
                            false
                        ),
                        "RowParallelLinear (bias=true): Bias gradients differ between rank 0 and rank {}",
                        i
                    );
                }
            }
        }

        Ok(())
    }
}
