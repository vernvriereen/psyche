use anyhow::Result;
use clap::Parser;
use psyche_core::{CosineLR, LearningRateScheduler};
use psyche_data_provider::{download_model_repo_sync, LocalDataProvider};
use psyche_modeling::{
    Batcher, CausalLM, CommunicatorId, Fp32GradientAccumulator, LlamaForCausalLM,
};
use rand::Rng;
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
}

fn train(
    repo_files: Vec<PathBuf>,
    tensor_parallelism: Option<(Arc<CommunicatorId>, usize, usize)>,
    args: Args,
    seed: [u8; 32],
) -> Result<()> {
    let dataset = LocalDataProvider::new_from_directory(
        &args.data_path,
        args.token_size.try_into()?,
        args.sequence_length,
        seed,
    )?;
    let rank = tensor_parallelism
        .as_ref()
        .map(|(_, rank, _)| *rank)
        .unwrap_or_default();
    let mut model = LlamaForCausalLM::from_pretrained(
        &repo_files,
        Some(Kind::BFloat16),
        None,
        tensor_parallelism.as_ref().map(|_| Device::Cuda(rank)),
        tensor_parallelism,
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
    let adamw: nn::AdamW = nn::AdamW {
        beta1: args.beta1,
        beta2: args.beta2,
        wd: args.weight_decay,
        eps: args.eps,
        amsgrad: false,
    };

    let mut opt = adamw.build(&model.variables, args.learning_rate)?;

    let mut index_to_name = HashMap::new();
    let named_variables = model.variables.variables().into_iter().collect::<Vec<_>>();

    for (index, variable) in opt.trainable_variables().iter().enumerate() {
        if let Some(var) = named_variables
            .iter()
            .find(|x| x.1.is_set_to(variable))
            .map(|x| x.0.clone())
        {
            index_to_name.insert(index, var);
        }
    }

    let grad_accum_steps = args.total_batch / args.micro_batch;
    let grad_accum_divisor = grad_accum_steps as f64;
    let mut grad_accum = Fp32GradientAccumulator::new(&opt.trainable_variables(), device);
    for step in 0..args.total_steps {
        let start_time = SystemTime::now();
        let lr = schedule.get_lr(step);
        opt.set_lr(lr);
        let mut avg_loss: f32 = 0.0;
        for _ in 0..grad_accum_steps {
            let (inputs, targets) = batch_iter.next().unwrap()?;
            let (_, loss) = model.forward(&inputs, Some(&targets), None);
            let loss = loss.unwrap() / grad_accum_divisor;
            loss.backward();
            let loss_value: f32 = loss.try_into()?;
            avg_loss += loss_value;
            grad_accum.accumulate_gradients();
        }
        grad_accum.apply_accumulation();

        if rank == 0 && args.optim_stats {
            let mut variables = opt.trainable_variables_with_sharding();
            for (index, (variable, _shard)) in variables.iter_mut().enumerate() {
                if let Some(name) = index_to_name.get(&index) {
                    let grad_energy: f64 = variable
                        .grad()
                        .norm_scalaropt_dtype(1, Kind::Float)
                        .try_into()
                        .unwrap();
                    println!("{name} {grad_energy}")
                }
            }
        }

        opt.clip_grad_norm(args.max_grad_norm);
        opt.step();
        opt.zero_grad();
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
    let seed: [u8; 32] = rand::thread_rng().gen();
    match args.tensor_parallelism {
        Some(0) | Some(1) | None => train(repo_files, None, args, seed)?,
        Some(world_size) => {
            let id = Arc::new(CommunicatorId::new());
            let threads = (0..world_size)
                .map(|rank| {
                    let repo_files = repo_files.clone();
                    let args = args.clone();
                    let id = id.clone();
                    std::thread::spawn(move || {
                        train(repo_files, Some((id, rank, world_size)), args, seed)
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
