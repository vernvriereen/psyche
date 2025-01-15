use anyhow::Result;
use core::fmt;
use iroh::{
    endpoint::{Connecting, Connection},
    protocol::ProtocolHandler,
};
use iroh_blobs::ticket::BlobTicket;
use psyche_core::BoxedFuture;
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Write};
use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    error::Error,
};
use tch::{Device, Kind, Tensor};
use tokio::sync::{mpsc::UnboundedSender, oneshot};
use tracing::info;

pub const ALPN: &[u8] = b"model-parameter-sharing/0";

#[derive(Debug)]
pub enum SharableModelParameterError {
    TchSerializeError(tch::TchError),
    InvalidUpdate,
    ParameterUnknown(String),
    ParameterAlreadyAdded,
    SerializationError(String),
    ParametersNotInitialized,
    ResponseChannelNotInitialized,
}

impl From<tch::TchError> for SharableModelParameterError {
    fn from(err: tch::TchError) -> Self {
        SharableModelParameterError::TchSerializeError(err)
    }
}

impl From<std::io::Error> for SharableModelParameterError {
    fn from(err: std::io::Error) -> Self {
        SharableModelParameterError::SerializationError(err.to_string())
    }
}

impl From<std::string::FromUtf8Error> for SharableModelParameterError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        SharableModelParameterError::SerializationError(err.to_string())
    }
}

impl fmt::Display for SharableModelParameterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SharableModelParameterError::TchSerializeError(err) => {
                write!(f, "Torch serialize error: {}", err)
            }
            SharableModelParameterError::InvalidUpdate => {
                write!(f, "The update of the sharable model parameters is invalid")
            }
            SharableModelParameterError::ParameterUnknown(unknown_param_name) => {
                write!(
                    f,
                    "Parameter with name {} is unknown",
                    unknown_param_name.to_string()
                )
            }
            SharableModelParameterError::ParameterAlreadyAdded => {
                write!(f, "The parameter was already added")
            }
            SharableModelParameterError::SerializationError(err) => {
                write!(f, "Serialization error: {}", err)
            }
            SharableModelParameterError::ParametersNotInitialized => {
                write!(f, "Parameters were not initialized")
            }
            SharableModelParameterError::ResponseChannelNotInitialized => {
                write!(f, "Response channel was not initialized")
            }
        }
    }
}

impl Error for SharableModelParameterError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SharableModelParameterError::TchSerializeError(err) => Some(err),
            SharableModelParameterError::InvalidUpdate => Some(self),
            SharableModelParameterError::ParameterUnknown(_unknown_parameter) => Some(self),
            SharableModelParameterError::ParameterAlreadyAdded => Some(self),
            SharableModelParameterError::SerializationError(_err) => Some(self),
            SharableModelParameterError::ParametersNotInitialized => Some(self),
            SharableModelParameterError::ResponseChannelNotInitialized => Some(self),
        }
    }
}

pub enum ParameterSharingMessage {
    Get(String, oneshot::Sender<BlobTicket>),
    Response(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
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

#[derive(Debug)]
pub struct ModelParameters {
    parameters: Option<HashMap<String, Tensor>>,
    tx_params_response: Option<oneshot::Sender<HashMap<String, Tensor>>>,
}
impl ModelParameters {
    pub fn empty() -> Self {
        Self {
            parameters: None,
            tx_params_response: None,
        }
    }

    pub fn update_parameters(
        &mut self,
        new_parameters: HashMap<String, Tensor>,
    ) -> Result<(), SharableModelParameterError> {
        let Some(parameters) = self.parameters.as_mut() else {
            self.parameters = Some(new_parameters);
            return Ok(());
        };

        // validate that both models have the same parameters
        let new_parameters_names: HashSet<_> = new_parameters.keys().cloned().collect();
        let parameters_names: HashSet<_> = parameters.keys().cloned().collect();
        if new_parameters_names != parameters_names {
            return Err(SharableModelParameterError::InvalidUpdate);
        }

        self.parameters = Some(new_parameters);
        Ok(())
    }

    pub fn get_transmittable_parameter(
        &self,
        param_name: &str,
    ) -> Result<TransmittableModelParameter, SharableModelParameterError> {
        let Some(parameters) = self.parameters.as_ref() else {
            return Err(SharableModelParameterError::ParametersNotInitialized);
        };

        let Some(parameter) = parameters.get(param_name) else {
            return Err(SharableModelParameterError::ParameterUnknown(
                param_name.to_string(),
            ));
        };

        let mut param_name_buffer = Vec::new();
        let mut param_value_buffer = Vec::new();

        param_name_buffer.write_all(param_name.as_bytes())?;
        parameter.save_to_stream(&mut param_value_buffer)?;

        let transmittable_parameter =
            TransmittableModelParameter::new(param_name_buffer, param_value_buffer);

        Ok(transmittable_parameter)
    }

    pub fn initialize_parameters(
        &mut self,
        param_names: &[String],
        tx_params_response: oneshot::Sender<HashMap<String, Tensor>>,
    ) {
        // Initialize the model parameter names with a dummy zero tensor.
        let mut parameters = HashMap::new();
        for param_name in param_names {
            parameters.insert(
                param_name.clone(),
                Tensor::zeros([1], (Kind::BFloat16, Device::Cpu)),
            );
        }
        self.parameters = Some(parameters);
        self.tx_params_response = Some(tx_params_response);
    }

    pub fn add_parameter(
        &mut self,
        parameter: TransmittableModelParameter,
    ) -> Result<(), SharableModelParameterError> {
        let Some(parameters) = self.parameters.as_mut() else {
            return Err(SharableModelParameterError::ParametersNotInitialized);
        };

        // Deserialize model parameter
        let param_name = String::from_utf8(parameter.param_name_bytes)?;
        let buf_reader = Cursor::new(parameter.param_value_bytes);
        let param_value = Tensor::load_from_stream(buf_reader)?;

        // Validate that the parameter does not already exist
        // This should be called only by a client that joins the run
        match parameters.entry(param_name.to_string()) {
            Entry::Occupied(mut param_entry) => {
                let param = param_entry.get_mut();
                if is_initialized(param) {
                    return Err(SharableModelParameterError::ParameterAlreadyAdded);
                }
                *param = param_value;
                Ok(())
            }
            Entry::Vacant(_) => {
                return Err(SharableModelParameterError::ParameterUnknown(
                    param_name.to_string(),
                ))
            }
        }
    }

    pub fn is_download_complete(&self) -> bool {
        let Some(parameters) = self.parameters.as_ref() else {
            return false;
        };

        parameters
            .iter()
            .all(|(_param_name, param_value)| is_initialized(param_value))
    }

    pub fn send_init_parameters(&mut self) -> Result<(), SharableModelParameterError> {
        if let Some(tx_params_response) = self.tx_params_response.take() {
            let Some(parameters) = self.parameters.take() else {
                return Err(SharableModelParameterError::ParametersNotInitialized);
            };
            tx_params_response.send(parameters).unwrap();
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
            let (tx_req, rx_req) = oneshot::channel::<BlobTicket>();
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

pub fn is_initialized(tensor: &Tensor) -> bool {
    tensor != &Tensor::zeros([1], (Kind::BFloat16, Device::Cpu))
}
