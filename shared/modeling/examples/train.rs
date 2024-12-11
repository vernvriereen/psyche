use anyhow::Result;
use clap::Parser;
use psyche_core::{CancellableBarrier, CosineLR, LearningRateScheduler};
use psyche_data_provider::{download_model_repo_sync, LocalDataProvider, Shuffle};
use psyche_modeling::{
    Batcher, CausalLM, CommunicatorId, Distro, Fp32GradientAccumulator, LlamaForCausalLM,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tch::nn::{self, OptimizerConfig};
use tch::{Device, Kind, Tensor};

#[derive(Parser, Debug, Clone)]
struct Args {
    #[arg(long, default_value = "emozilla/llama2-215m-init")]
    model: String,

    #[arg(long, default_value = "data")]
    data_path: String,

    #[arg(long, default_value_t = 2048)]
    sequence_length: usize,

    #[arg(long, default_value_t = 2)]
    token_size: usize,

    #[arg(long, default_value_t = 8)]
    micro_batch: usize,

    #[arg(long, default_value_t = 64)]
    total_batch: usize,

    #[arg(long, default_value_t = 0.9)]
    beta1: f64,

    #[arg(long, default_value_t = 0.95)]
    beta2: f64,

    #[arg(long, default_value_t = 0.1)]
    weight_decay: f64,

    #[arg(long, default_value_t = 1e-8)]
    eps: f64,

    #[arg(long, default_value_t = 4e-4)]
    learning_rate: f64,

    #[arg(long, default_value_t = 500)]
    warmup_steps: u32,

    #[arg(long, default_value_t = 25000)]
    total_steps: u32,

    #[arg(long, default_value_t = 1.0)]
    max_grad_norm: f64,

    #[arg(long)]
    tensor_parallelism: Option<usize>,

    #[arg(long, default_value_t = false)]
    optim_stats: bool,

    #[arg(long, default_value_t = false)]
    cpu: bool,

    #[arg(long, default_value_t = false)]
    print_tensors: bool,

    #[arg(long, default_value_t = false)]
    grad_accum_in_fp32: bool,

    #[arg(long, default_value_t = 64)]
    compression_chunk: i64,

    #[arg(long, default_value_t = 4)]
    compression_topk: i64,

    #[arg(long, default_value_t = 0.999)]
    compression_decay: f64,

    #[arg(long, default_value_t = false)]
    distro: bool,
}

fn train(
    repo_files: Vec<PathBuf>,
    tensor_parallelism: Option<(Arc<CommunicatorId>, usize, usize, Arc<CancellableBarrier>)>,
    args: Args,
) -> Result<()> {
    println!(
        "starting training run: model {}, data_path {}, sequence_length {}, token_size {}, micro_batch {}, total_batch {}, beta1 {:.9}, beta2 {:.9}, weight_decay {:.9}, eps {:.9}, learning_rate {:.9}, warmup_steps {}, total_steps {}, max_grad_norm {:.9}, print_tensors {}, grad_accum_in_fp32 {}, compression_chunk {}, compression_topk {}, compression_decay {}, distro {}",
        args.model,
        args.data_path,
        args.sequence_length,
        args.token_size,
        args.micro_batch,
        args.total_batch,
        args.beta1,
        args.beta2,
        args.weight_decay,
        args.eps,
        args.learning_rate,
        args.warmup_steps,
        args.total_steps,
        args.max_grad_norm,
        args.print_tensors,
        args.grad_accum_in_fp32,
        args.compression_chunk,
        args.compression_topk,
        args.compression_decay,
        args.distro,
    );

    let dataset = LocalDataProvider::new_from_directory(
        &args.data_path,
        args.token_size.try_into()?,
        args.sequence_length,
        Shuffle::DontShuffle,
    )?;
    let rank = tensor_parallelism
        .as_ref()
        .map(|(_, rank, _, _)| *rank)
        .unwrap_or_default();
    let mut model = LlamaForCausalLM::from_pretrained(
        &repo_files,
        Some(Kind::BFloat16),
        None,
        args.cpu
            .then_some(Device::Cpu)
            .or(tensor_parallelism.as_ref().map(|_| Device::Cuda(rank))),
        tensor_parallelism
            .as_ref()
            .map(|(id, rank, size, _)| (id.clone(), *rank, *size)),
        None,
    )?;
    let device = model.device();
    let iter = dataset.into_iter().map(|tokens| {
        Ok((
            Tensor::from_slice(&tokens).to(device),
            Tensor::from_slice(&tokens).to(device),
        ))
    });
    let mut batch_iter = Batcher::new_r2(iter).batch_size(args.micro_batch);
    let schedule = CosineLR::new(
        args.learning_rate,
        args.warmup_steps,
        0.0,
        args.total_steps,
        args.learning_rate / 10.0,
    );

    let mut adamw = match args.distro {
        false => {
            let adamw: nn::AdamW = nn::AdamW {
                beta1: args.beta1,
                beta2: args.beta2,
                wd: args.weight_decay,
                eps: args.eps,
                amsgrad: false,
            };

            Some(adamw.build(&model.variables, args.learning_rate)?)
        }
        true => None,
    };

    let mut distro = match args.distro {
        true => Some(Distro::new(
            &model.variables,
            args.compression_decay,
            args.compression_chunk,
            0.0,
            model.comm.clone(),
        )),
        false => None,
    };

    let mut variables = match &adamw {
        Some(adamw) => adamw.trainable_variables_with_sharding(),
        None => distro.as_ref().unwrap().trainable_variables_with_sharding(),
    };

    let mut index_to_name = HashMap::new();
    let named_variables = model.variables.variables().into_iter().collect::<Vec<_>>();

    for (index, (variable, _)) in variables.iter().enumerate() {
        if let Some(var) = named_variables
            .iter()
            .find(|x| x.1.is_set_to(variable))
            .map(|x| x.0.clone())
        {
            index_to_name.insert(index, var);
        }
    }

    println!("Done loading, starting training.");
    let grad_accum_steps = args.total_batch / args.micro_batch;
    let grad_accum_divisor = grad_accum_steps as f64;
    let mut grad_accum = match args.grad_accum_in_fp32 {
        true => Some(Fp32GradientAccumulator::new(
            &variables
                .iter()
                .map(|(variable, _)| variable.shallow_clone())
                .collect::<Vec<_>>(),
            device,
        )),
        false => None,
    };
    for step in 0..args.total_steps {
        let start_time = SystemTime::now();
        let lr = schedule.get_lr(step + 1);
        if let Some(adamw) = &mut adamw {
            adamw.set_lr(lr);
        }
        let mut avg_loss: f32 = 0.0;
        for i in 0..grad_accum_steps {
            let (inputs, targets) = batch_iter.next().unwrap()?;
            if let Some((_, _, _, barrier)) = tensor_parallelism.as_ref() {
                barrier.wait().expect("barrier fail");
            }
            let (_, loss) = model.forward(&inputs, Some(&targets), None);
            let loss = loss.expect("no loss!") / grad_accum_divisor;
            if let Some((_, _, _, barrier)) = tensor_parallelism.as_ref() {
                barrier.wait().expect("barrier fail");
            }
            if args.print_tensors {
                println!(
                    "step {step} grad accum step {i} causal LM forward loss: {}",
                    tp(&loss)
                );
            }
            loss.backward();
            if let Some((_, _, _, barrier)) = tensor_parallelism.as_ref() {
                barrier.wait().expect("barrier fail");
            }

            let loss_value: f32 = loss.try_into()?;
            avg_loss += loss_value;

            if let Some(grad_accum) = &mut grad_accum {
                grad_accum.accumulate_gradients();
            }
        }
        if let Some(grad_accum) = &mut grad_accum {
            grad_accum.apply_accumulation();
        }

        if args.print_tensors || (rank == 0 && args.optim_stats) {
            let mut outputs: Vec<(&String, &mut Tensor)> = Vec::new();
            for (index, (variable, _shard)) in variables.iter_mut().enumerate() {
                if let Some(name) = index_to_name.get(&index) {
                    outputs.push((name, variable));
                }
            }
            outputs.sort_by_key(|(name, _)| (*name).clone());
            for (name, variable) in outputs {
                if args.print_tensors {
                    println!(
                        "step {step} causal LM backward variable: {} {}",
                        name,
                        tp(&variable.grad())
                    );
                }
                if args.optim_stats && rank == 0 {
                    let grad_energy: f64 = variable
                        .grad()
                        .norm_scalaropt_dtype(1, Kind::Float)
                        .try_into()
                        .unwrap();
                    println!("{name} {grad_energy}")
                }
            }
        }

        if let Some(adamw) = &mut adamw {
            adamw.clip_grad_norm(args.max_grad_norm);
            adamw.step();
            adamw.zero_grad();
        };

        if let Some(distro) = &mut distro {
            distro.clip_grad_norm(args.max_grad_norm);
            let results = distro.generate(lr, 1.0, args.compression_topk, false, args.optim_stats);
            distro.apply(&[results], lr);
            distro.zero_grad();
        }

        if let Some(grad_accum) = &mut grad_accum {
            grad_accum.zero_grad();
        }
        let duration = SystemTime::now()
            .duration_since(start_time)
            .unwrap()
            .as_secs_f32();

        if rank == 0 {
            println!(
                "step: {}, duration: {:.1}, lr: {:.1e}, loss: {:.4}",
                step, duration, lr, avg_loss
            );
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let repo_files = download_model_repo_sync(&args.model.clone(), None, None, None, false)?;
    match args.tensor_parallelism {
        Some(0) | Some(1) | None => train(repo_files, None, args)?,
        Some(world_size) => {
            let id = Arc::new(CommunicatorId::new());
            let barrier = CancellableBarrier::new(world_size);
            let threads = (0..world_size)
                .map(|rank| {
                    let repo_files = repo_files.clone();
                    let args = args.clone();
                    let id = id.clone();
                    let barrier = barrier.clone();
                    std::thread::spawn(move || {
                        train(repo_files, Some((id, rank, world_size, barrier)), args)
                    })
                })
                .collect::<Vec<_>>();
            for thread in threads {
                thread.join().unwrap()?;
            }
        }
    }
    Ok(())
}

fn tp(tensor: &Tensor) -> String {
    let vals = tensor
        .flatten(0, -1)
        .iter::<f64>()
        .unwrap()
        .collect::<Vec<f64>>()
        .into_iter()
        .map(|f| format!("{f:.9}"))
        .collect::<Vec<_>>()
        .join("\n");

    let kind = match tensor.kind() {
        Kind::Float => "float32",
        Kind::Double => "float64",
        Kind::Float8e4m3fn => "float8_e4m3fn",
        Kind::Float8e4m3fnuz => "float8_e4m3fnuz",
        Kind::Float8e5m2 => "float8_e5m2",
        Kind::Float8e5m2fnuz => "float8_e5m2fnuz",
        Kind::Half => "float16",
        Kind::BFloat16 => "bfloat16",
        Kind::Uint8 => "uint8",
        Kind::UInt16 => "uint16",
        Kind::UInt32 => "uint32",
        Kind::UInt64 => "uint64",
        Kind::Int8 => "int8",
        Kind::Int16 => "int16",
        Kind::Int => "int32",
        Kind::Int64 => "int64",
        Kind::ComplexHalf => "complex32",
        Kind::ComplexFloat => "complex64",
        Kind::ComplexDouble => "complex128",
        Kind::QUInt8 => "quint8",
        Kind::QInt8 => "qint8",
        Kind::QInt32 => "qint32",
        Kind::Bool => "bool",
        Kind::QUInt4x2 => "quint4x2",
        Kind::QUInt2x4 => "quint2x4",
        Kind::Bits1x8 => "bits1x8",
        Kind::Bits2x4 => "bits2x4",
        Kind::Bits4x2 => "bits4x2",
        Kind::Bits8 => "bits8",
        Kind::Bits16 => "bits16",
        Kind::UInt1 => "uint1",
        Kind::UInt2 => "uint2",
        Kind::UInt3 => "uint3",
        Kind::UInt4 => "uint4",
        Kind::UInt5 => "uint5",
        Kind::UInt6 => "uint6",
        Kind::UInt7 => "uint7",
    };

    let size = tensor
        .size()
        .iter()
        .map(|d| format!("{d}"))
        .collect::<Vec<_>>()
        .join(",");

    format!("[ torch.{kind}{{{size}}} ]\n{vals}")
}
