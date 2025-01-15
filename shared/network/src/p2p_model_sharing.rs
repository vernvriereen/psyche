use anyhow::Result;
use core::fmt;
use iroh::{
    endpoint::{Connecting, Connection},
    protocol::ProtocolHandler,
};
use iroh_blobs::ticket::BlobTicket;
use psyche_core::BoxedFuture;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
};
use tch::Tensor;
use tokio::sync::{mpsc::UnboundedSender, oneshot};

pub const ALPN: &[u8] = b"model-parameter-sharing/0";

#[derive(Debug)]
pub enum SharableModelParameterError {
    TchSerializeError(tch::TchError),
    ParameterNotFound(String),
    InvalidUpdate,
}

impl From<tch::TchError> for SharableModelParameterError {
    fn from(err: tch::TchError) -> Self {
        SharableModelParameterError::TchSerializeError(err)
    }
}

impl fmt::Display for SharableModelParameterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SharableModelParameterError::TchSerializeError(err) => {
                write!(f, "Torch serialize error: {}", err)
            }
            SharableModelParameterError::ParameterNotFound(name) => {
                write!(f, "Parameter with name {name} not found")
            }
            SharableModelParameterError::InvalidUpdate => {
                write!(f, "The update of the sharable model parameters is invalid")
            }
        }
    }
}

impl Error for SharableModelParameterError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SharableModelParameterError::TchSerializeError(err) => Some(err),
            SharableModelParameterError::InvalidUpdate => Some(self),
            SharableModelParameterError::ParameterNotFound(_name) => Some(self),
        }
    }
}

pub enum ParameterSharingMessage {
    Get(String, oneshot::Sender<BlobTicket>),
    Response(String),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TransmittableModelParameter(Vec<u8>);

#[derive(Debug)]
pub struct ModelParameters(HashMap<String, Tensor>);
impl ModelParameters {
    pub fn empty() -> Self {
        Self(HashMap::new())
    }

    pub fn update_parameters(
        &mut self,
        new_parameters: HashMap<String, Tensor>,
    ) -> Result<(), SharableModelParameterError> {
        if self.0.is_empty() {
            self.0 = new_parameters;
            return Ok(());
        }

        // validate that both models have the same parameters
        let new_parameters_names: HashSet<_> = new_parameters.keys().cloned().collect();
        let parameters_names: HashSet<_> = self.0.keys().cloned().collect();
        if new_parameters_names != parameters_names {
            return Err(SharableModelParameterError::InvalidUpdate);
        }

        self.0 = new_parameters;
        Ok(())
    }

    pub fn get_transmittable_parameter(
        &self,
        parameter_name: &str,
    ) -> Result<TransmittableModelParameter, SharableModelParameterError> {
        let Some(parameter) = self.0.get(parameter_name) else {
            return Err(SharableModelParameterError::ParameterNotFound(
                parameter_name.to_string(),
            ));
        };
        let mut buffer = Vec::new();
        parameter.save_to_stream(&mut buffer)?;
        let transmittable_parameter = TransmittableModelParameter(buffer);
        Ok(transmittable_parameter)
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
