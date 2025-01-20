use anyhow::Result;
use iroh::{
    endpoint::{Connecting, Connection},
    protocol::ProtocolHandler,
};
use iroh_blobs::ticket::BlobTicket;
use psyche_core::BoxedFuture;
use std::collections::{hash_map::Entry, HashMap, HashSet};
use std::io::{Cursor, Write};
use tch::Tensor;
use thiserror::Error;
use tokio::sync::{mpsc::UnboundedSender, oneshot};
use tracing::warn;

pub const ALPN: &[u8] = b"model-parameter-sharing/0";
pub const REQUEST_PARAMETER_TIMEOUT_SECS: u64 = 3;

#[derive(Error, Debug, serde::Serialize, serde::Deserialize)]
pub enum SharableModelParameterError {
    #[error("Torch serialize error: {0}")]
    TchSerializeError(String),
    #[error("The update of the sharable model parameters is invalid")]
    InvalidUpdate,
    #[error("Parameter with name {0} is unknown")]
    ParameterUnknown(String),
    #[error("The parameter was already added")]
    ParameterAlreadyAdded,
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Parameters were not initialized")]
    ParametersNotInitialized,
    #[error("Parameter {0} is known but was not yet initialized")]
    ParameterNotInitialized(String),
    #[error("Response channel was not initialized")]
    ResponseChannelNotInitialized,
    #[error("Connection IO error: {0}")]
    ConnectionIOError(String),
    #[error("Could not decode UTF-8 string of model parameter name: {0}")]
    DecodeParameterNameError(String),
}

pub enum ParameterSharingMessage {
    Get(
        String,
        oneshot::Sender<Result<BlobTicket, SharableModelParameterError>>,
    ),
    Response(String),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct TransmittableModelParameter {
    param_name_bytes: Vec<u8>,
    param_value_bytes: Vec<u8>,
}

impl TransmittableModelParameter {
    fn new(param_name_bytes: Vec<u8>, param_value_bytes: Vec<u8>) -> Self {
        Self {
            param_name_bytes,
            param_value_bytes,
        }
    }
}

// This data structure is the one responsible of storing the model
// parameters for sharing them to other peers via p2p, as well as
// storing them while parameters are downloaded from other peers.
#[derive(Debug)]
pub struct ModelParameters {
    parameters: Option<HashMap<String, Option<Tensor>>>,
    tx_params_response: Option<oneshot::Sender<HashMap<String, Tensor>>>,
}

// These impls are methods called by both the sharing model peers and the ones
// that download
impl ModelParameters {
    pub fn empty() -> Self {
        Self {
            parameters: None,
            tx_params_response: None,
        }
    }
}

// These impls on the `ModelParameters` struct are the ones called by the
// peers that are in charge of sharing the parameters to the newly joined ones.
impl ModelParameters {
    pub fn update_parameters(
        &mut self,
        new_parameters: HashMap<String, Tensor>,
    ) -> Result<(), SharableModelParameterError> {
        let Some(parameters) = self.parameters.as_mut() else {
            let mut parameters = HashMap::new();
            let new_parameters = new_parameters;
            new_parameters.into_iter().for_each(|(param_name, tensor)| {
                parameters.insert(param_name, Some(tensor));
            });
            self.parameters = Some(parameters);
            return Ok(());
        };

        // validate that both models have the same parameters
        let new_parameters_names: HashSet<_> = new_parameters.keys().cloned().collect();
        let parameters_names: HashSet<_> = parameters.keys().cloned().collect();
        if new_parameters_names != parameters_names {
            return Err(SharableModelParameterError::InvalidUpdate);
        }

        let mut parameters = HashMap::new();
        let new_parameters = new_parameters;
        new_parameters.into_iter().for_each(|(param_name, tensor)| {
            parameters.insert(param_name, Some(tensor));
        });
        self.parameters = Some(parameters);
        Ok(())
    }

    pub fn get_transmittable_parameter(
        &self,
        param_name: &str,
    ) -> Result<TransmittableModelParameter, SharableModelParameterError> {
        let Some(parameters) = self.parameters.as_ref() else {
            return Err(SharableModelParameterError::ParametersNotInitialized);
        };

        match parameters.get(param_name) {
            Some(Some(parameter)) => {
                let mut param_name_buffer = Vec::new();
                let mut param_value_buffer = Vec::new();

                param_name_buffer
                    .write_all(param_name.as_bytes())
                    .map_err(|e| SharableModelParameterError::ConnectionIOError(e.to_string()))?;
                parameter
                    .save_to_stream(&mut param_value_buffer)
                    .map_err(|e| SharableModelParameterError::TchSerializeError(e.to_string()))?;

                let transmittable_parameter =
                    TransmittableModelParameter::new(param_name_buffer, param_value_buffer);

                Ok(transmittable_parameter)
            }
            _ => {
                warn!("Paramater {param_name:?} not initialized");
                Err(SharableModelParameterError::ParameterUnknown(
                    param_name.to_string(),
                ))
            }
        }
    }
}

// These impls on the `ModelParameters` struct are the ones called by the
// new peers that are joining a run and have to download parameters from peers
// that are sharing them.
impl ModelParameters {
    // Initialize the model parameter names. This is important to know when
    // all model parameters have been downloaded from other peers.
    pub fn initialize_parameters(
        &mut self,
        param_names: &[String],
        tx_params_response: oneshot::Sender<HashMap<String, Tensor>>,
    ) {
        // Initialize the model parameter names with None.
        let mut parameters = HashMap::new();
        for param_name in param_names {
            parameters.insert(param_name.clone(), None);
        }
        self.parameters = Some(parameters);
        self.tx_params_response = Some(tx_params_response);
    }

    // Add new parameter downloaded from another peer
    pub fn add_parameter(
        &mut self,
        parameter: TransmittableModelParameter,
    ) -> Result<(), SharableModelParameterError> {
        let Some(parameters) = self.parameters.as_mut() else {
            return Err(SharableModelParameterError::ParametersNotInitialized);
        };

        // Deserialize model parameter
        let param_name = String::from_utf8(parameter.param_name_bytes)
            .map_err(|e| SharableModelParameterError::DecodeParameterNameError(e.to_string()))?;
        let buf_reader = Cursor::new(parameter.param_value_bytes);
        let param_value = Tensor::load_from_stream(buf_reader)
            .map_err(|e| SharableModelParameterError::TchSerializeError(e.to_string()))?;

        // Validate that the parameter does not already exist
        // This should be called only by a client that joins the run
        match parameters.entry(param_name.to_string()) {
            Entry::Occupied(mut param_entry) => {
                let param = param_entry.get_mut();
                if param.is_some() {
                    return Err(SharableModelParameterError::ParameterAlreadyAdded);
                }
                *param = Some(param_value);
                Ok(())
            }
            Entry::Vacant(_) => Err(SharableModelParameterError::ParameterUnknown(
                param_name.to_string(),
            )),
        }
    }

    // Utility function that is used to know when we have downloaded all
    // model parameters from the other peers
    pub fn is_download_complete(&self) -> bool {
        let Some(parameters) = self.parameters.as_ref() else {
            return false;
        };

        parameters
            .iter()
            .all(|(_param_name, param_value)| param_value.is_some())
    }

    // Once all parameters have been downloaded, this function is called to send them
    // to the initialization task, so that the model can be loaded
    pub fn send_init_parameters(&mut self) -> Result<(), SharableModelParameterError> {
        if let Some(tx_params_response) = self.tx_params_response.take() {
            let Some(parameters) = self.parameters.take() else {
                return Err(SharableModelParameterError::ParametersNotInitialized);
            };

            let mut parameters_to_send = HashMap::new();
            for (param_name, parameter) in parameters.into_iter() {
                let Some(tensor) = parameter else {
                    // This error should never really happen, but checking just in case
                    // something goes really wrong
                    return Err(SharableModelParameterError::ParameterNotInitialized(
                        param_name,
                    ));
                };
                parameters_to_send.insert(param_name, tensor);
            }
            tx_params_response.send(parameters_to_send).unwrap();
            return Ok(());
        }
        Err(SharableModelParameterError::ResponseChannelNotInitialized)
    }
}

#[derive(Debug, Clone)]
pub struct ModelParameterSharing {
    tx_model_parameter_req: UnboundedSender<ParameterSharingMessage>,
}

impl ModelParameterSharing {
    pub fn new(tx_model_parameter_req: UnboundedSender<ParameterSharingMessage>) -> Self {
        Self {
            tx_model_parameter_req,
        }
    }
    pub(crate) fn _accept_connection(
        connection: Connection,
        tx_model_parameter_req: UnboundedSender<ParameterSharingMessage>,
    ) -> BoxedFuture<Result<()>> {
        Box::pin(async move {
            let (mut send, mut recv) = connection.accept_bi().await?;
            let parameter_request_bytes = recv.read_to_end(1000).await?;
            let Ok(parameter_request) = String::from_utf8(parameter_request_bytes) else {
                send.write_all(b"Invalid parameter request").await?;
                return Ok(());
            };

            // Create channel for requesting the model parameter to the client backend
            // and add a new blob for it
            let (tx_req, rx_req) =
                oneshot::channel::<Result<BlobTicket, SharableModelParameterError>>();
            let request = ParameterSharingMessage::Get(parameter_request, tx_req);
            tx_model_parameter_req.send(request)?;

            // Receive the blob ticket and forward it to the requesting client
            let parameter_blob_ticket = rx_req.await?;
            let data = postcard::to_stdvec(&parameter_blob_ticket)?;
            send.write_all(&data).await?;
            send.finish()?;

            // Wait until the remote closes the connection, which it does once it
            // received the response.
            connection.closed().await;

            Ok(())
        })
    }

    pub fn accept_connection(&self, connection: Connection) -> BoxedFuture<Result<()>> {
        let tx_model_parameter_req = self.tx_model_parameter_req.clone();
        Box::pin(async move { Self::_accept_connection(connection, tx_model_parameter_req).await })
    }
}

impl ProtocolHandler for ModelParameterSharing {
    fn accept(&self, connecting: Connecting) -> BoxedFuture<Result<()>> {
        let tx_model_parameter_req = self.tx_model_parameter_req.clone();
        Box::pin(async move {
            let connection = connecting.await?;
            Self::_accept_connection(connection, tx_model_parameter_req).await
        })
    }
}
