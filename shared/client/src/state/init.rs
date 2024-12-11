use std::{path::PathBuf, sync::Arc};

use psyche_coordinator::{model, Coordinator, HealthChecks, Witness};
use psyche_data_provider::{download_model_repo_async, DataProviderTcpClient};
use psyche_modeling::{
    auto_tokenizer, AutoTokenizerError, CommunicatorId, ConcreteCausalLM, DummyModel,
    LlamaForCausalLM, LoadLlamaForCausalLMError,
};
use psyche_network::{BlobTicket, NetworkableNodeIdentity};
use tch::{Device, Kind};
use thiserror::Error;
use tokenizers::Tokenizer;
use tokio::{
    io,
    sync::mpsc::Sender,
    task::{JoinError, JoinHandle},
};
use tracing::info;

use crate::{
    fetch_data::DataFetcher,
    trainer::{ParallelModels, Trainer},
    WandBInfo,
};

use super::{
    cooldown::CooldownStepMetadata, evals::EvalRunner, stats::StatsLogger, steps::StepStateMachine,
    train::TrainingStepMetadata, types::DistroBroadcastAndPayload, warmup::WarmupStepMetadata,
    witness::WitnessStepMetadata, CheckpointConfig,
};

pub struct RunInitConfig<T: NetworkableNodeIdentity> {
    // identity for connecting to the data server
    pub identity: T,
    pub private_key: T::PrivateKey,

    // model & dataload
    pub hub_read_token: Option<String>,
    pub data_parallelism: usize,
    pub tensor_parallelism: usize,
    pub micro_batch_size: Option<usize>,
    pub optim_stats_every_n_steps: Option<u32>,
    pub grad_accum_in_fp32: bool,

    // evaluation
    pub eval_task_max_docs: Option<usize>,
    pub eval_tasks: Vec<psyche_eval::Task>,

    // logging
    pub wandb_info: Option<WandBInfo>,

    // debugging
    pub write_gradients_dir: Option<PathBuf>,

    // checkpointing
    pub checkpoint_config: Option<CheckpointConfig>,
}

#[derive(Debug, Error)]
pub enum InitRunError {
    #[error("No model provided in Coordinator state, nothing to do.")]
    NoModel,

    #[error("Model is Ephemeral, it's impossible to join this run.")]
    ModelIsEphemeral,

    #[error("failed to read local model info: {0}")]
    LocalModelLoad(#[from] io::Error),

    #[error("failed to read HF model info: {0}")]
    HfModelLoad(#[from] hf_hub::api::tokio::ApiError),

    #[error("model loading thread crashed")]
    ModelLoadingThreadCrashed(JoinError),

    #[error("failed to load model: {0}")]
    ModelLoad(#[from] LoadLlamaForCausalLMError),

    #[error("Couldn't load tokenizer: {0}")]
    TokenizerLoad(#[from] AutoTokenizerError),

    // TODO refactor data provider for real errors
    #[error("Couldn't initialize data provider")]
    DataProviderConnect(anyhow::Error),

    #[error("wandb setup thread crashed")]
    WandbThreadCrashed(JoinError),

    #[error("wandb failed to create run")]
    WandbLoad(#[from] wandb::ApiError),
}

struct RawLoadedModel {
    models: Vec<Box<dyn ConcreteCausalLM>>,
    tokenizer: Arc<Tokenizer>,
    eval_runner: EvalRunner,
    checkpoint_extra_files: Vec<PathBuf>,
}

pub struct RunInitConfigAndIO<T: NetworkableNodeIdentity> {
    pub init_config: RunInitConfig<T>,

    pub tx_witness: Sender<Witness>,
    pub tx_health_check: Sender<HealthChecks>,
    pub tx_checkpoint: Sender<model::Checkpoint>,
    pub tx_distro_result: Sender<DistroBroadcastAndPayload>,
    pub tx_request_download: Sender<BlobTicket>,
}

impl<T: NetworkableNodeIdentity> RunInitConfigAndIO<T> {
    /// Call this on first warmup - when we need to enter the run, we have to load the model, conenct to the data server, etc
    pub async fn init_run(
        self,
        state: Coordinator<T>,
    ) -> Result<StepStateMachine<T>, InitRunError> {
        let Self {
            init_config,
            tx_witness,
            tx_health_check,
            tx_checkpoint,
            tx_distro_result,
            tx_request_download,
        } = self;

        let model::Model::LLM(llm) = state.model.clone().ok_or(InitRunError::NoModel)?;

        let data_future = match &llm.data_location {
            model::LLMTrainingDataLocation::Server(data_server) => DataProviderTcpClient::connect(
                data_server,
                init_config.identity.clone(),
                init_config.private_key,
            ),
            model::LLMTrainingDataLocation::Local(_) => todo!(),
        };

        let model_future: JoinHandle<Result<RawLoadedModel, InitRunError>> = match &llm.architecture
        {
            model::LLMArchitecture::HfLlama => match &llm.checkpoint {
                model::Checkpoint::Hub(hub_repo) => {
                    let hub_repo = hub_repo.clone();
                    tokio::spawn(async move {
                        let potential_local_path = PathBuf::from(hub_repo.repo_id.clone());
                        let model_is_local = match hub_repo.revision.is_none()
                            && tokio::fs::try_exists(potential_local_path.clone())
                                .await
                                .unwrap_or_default()
                        {
                            true => {
                                let mut ret = Vec::new();
                                let mut read_dir =
                                    tokio::fs::read_dir(potential_local_path).await?;
                                while let Some(dir_entry) = read_dir.next_entry().await? {
                                    ret.push(dir_entry.path())
                                }
                                ret
                            }
                            false => {
                                info!("Downloading {}", hub_repo.repo_id);
                                download_model_repo_async(
                                    hub_repo.repo_id.clone(),
                                    hub_repo.revision,
                                    None,
                                    init_config.hub_read_token,
                                    None,
                                    false,
                                )
                                .await?
                            }
                        };
                        let repo_files = model_is_local;
                        let checkpoint_extra_files = repo_files
                            .iter()
                            .filter(|file| {
                                file.ends_with("config.json")
                                    || file.ends_with("tokenizer.json")
                                    || file.ends_with("tokenizer_config.json")
                                    || file.ends_with("special_tokens_map.json")
                                    || file.ends_with("generation_config.json")
                            })
                            .cloned()
                            .collect();
                        info!("Loading {}", hub_repo.repo_id);
                        let mut futures = Vec::with_capacity(
                            init_config.data_parallelism * init_config.tensor_parallelism,
                        );
                        for dp in 0..init_config.data_parallelism {
                            let communicator_id = match init_config.tensor_parallelism {
                                1 => None,
                                _ => Some(Arc::new(CommunicatorId::new())),
                            };
                            for tp in 0..init_config.tensor_parallelism {
                                let tensor_parallelism_world =
                                    communicator_id.as_ref().map(|communicator_id| {
                                        (
                                            communicator_id.clone(),
                                            tp,
                                            init_config.tensor_parallelism,
                                        )
                                    });
                                let repo_files = repo_files.clone();
                                futures.push(tokio::task::spawn_blocking(move || {
                                    // let this run on CPU if tp is 1
                                    let device = if init_config.tensor_parallelism == 1 {
                                        if dp == 0 {
                                            Device::cuda_if_available()
                                        } else {
                                            Device::Cuda(dp)
                                        }
                                    } else {
                                        Device::Cuda(dp * init_config.tensor_parallelism + tp)
                                    };
                                    LlamaForCausalLM::from_pretrained(
                                        &repo_files,
                                        Some(Kind::BFloat16),
                                        None,
                                        Some(device),
                                        tensor_parallelism_world,
                                        Some(llm.max_seq_len as usize),
                                    )
                                }));
                            }
                        }
                        let tokenizer = Arc::new(auto_tokenizer(&repo_files)?);
                        let eval_runner = EvalRunner::new(
                            init_config.eval_tasks,
                            tokenizer.clone(),
                            init_config.eval_task_max_docs,
                            init_config.data_parallelism,
                        );

                        let mut models: Vec<Box<dyn ConcreteCausalLM>> = Vec::new();
                        for future in futures {
                            models.push(Box::new(
                                future
                                    .await
                                    .map_err(InitRunError::ModelLoadingThreadCrashed)??,
                            ));
                        }
                        info!(
                            "Loaded {} onto {} gpu(s) (dp={},tp={})",
                            hub_repo.repo_id,
                            init_config.data_parallelism * init_config.tensor_parallelism,
                            init_config.data_parallelism,
                            init_config.tensor_parallelism
                        );
                        Ok(RawLoadedModel {
                            models,
                            tokenizer,
                            eval_runner,
                            checkpoint_extra_files,
                        })
                    })
                }
                model::Checkpoint::Ephemeral => return Err(InitRunError::ModelIsEphemeral),
            },
        };

        let wandb_future: JoinHandle<Result<Option<wandb::Run>, wandb::ApiError>> = tokio::spawn({
            let run_id = state.run_id.clone();
            async move {
                match init_config.wandb_info {
                    Some(wandb_info) => {
                        let wandb =
                            wandb::WandB::new(wandb::BackendOptions::new(wandb_info.api_key));
                        let mut run_info = wandb::RunInfo::new(wandb_info.project)
                            .name(wandb_info.run)
                            .config((
                                ("data_indicies_per_batch", state.data_indicies_per_batch),
                                ("batches_per_round", state.batches_per_round),
                                ("total_steps", state.total_steps),
                                ("rounds_per_epoch", state.rounds_per_epoch),
                                ("run_id", run_id),
                            ));
                        if let Some(entity) = wandb_info.entity {
                            run_info = run_info.entity(entity);
                        }
                        if let Some(group) = wandb_info.group {
                            run_info = run_info.group(group);
                        }
                        Ok(Some(wandb.new_run(run_info.build()?).await?))
                    }
                    None => Ok(None),
                }
            }
        });

        let (data, models, wandb_run) = tokio::join!(data_future, model_future, wandb_future);
        let RawLoadedModel {
            models,
            tokenizer,
            checkpoint_extra_files,
            eval_runner,
        } = models.map_err(InitRunError::ModelLoadingThreadCrashed)??;

        let mut tp_models: Vec<Vec<Box<dyn ConcreteCausalLM>>> = Vec::new();
        for model in models {
            if tp_models
                .last()
                .map(|x: &ParallelModels| x.len() == init_config.tensor_parallelism)
                .unwrap_or(true)
            {
                tp_models.push(Vec::with_capacity(init_config.tensor_parallelism));
            }
            tp_models.last_mut().unwrap().push(model);
        }

        // TODO add data fetching for verifying, too..
        let data_provider = data.map_err(InitRunError::DataProviderConnect)?;

        let data_fetcher = DataFetcher::new(data_provider, init_config.data_parallelism * 2);

        let trainers = tp_models
            .into_iter()
            .map(|models| {
                Trainer::new(
                    models,
                    llm.lr_schedule.into(),
                    llm.optimizer,
                    init_config
                        .micro_batch_size
                        .unwrap_or(state.data_indicies_per_batch as usize),
                    init_config.optim_stats_every_n_steps,
                    init_config.grad_accum_in_fp32,
                    Some(state.step),
                )
            })
            .collect();

        let wandb_run = wandb_run.map_err(InitRunError::WandbThreadCrashed)??;

        let stats_logger = StatsLogger::new(tokenizer, eval_runner.clone(), wandb_run);

        let warmup = WarmupStepMetadata {
            eval_runner: eval_runner.clone(),
        };

        let training = TrainingStepMetadata {
            data_fetcher,
            identity: init_config.identity.clone(),
            write_gradients_dir: init_config.write_gradients_dir,
            tx_health_check,
            tx_distro_result,

            eval_runner: eval_runner.clone(),
        };

        let witness = WitnessStepMetadata {
            eval_runner: eval_runner.clone(),
            identity: init_config.identity.clone(),
            tx_witness: tx_witness.clone(),
        };

        let cooldown = CooldownStepMetadata::new(
            tx_checkpoint,
            init_config.checkpoint_config,
            checkpoint_extra_files,
            eval_runner,
        );

        Ok(StepStateMachine::new(
            init_config.identity,
            warmup,
            training,
            witness,
            cooldown,
            trainers,
            state,
            tx_request_download,
            tx_witness,
            stats_logger,
        ))
    }
}
