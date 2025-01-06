use core::fmt;
use std::{
    collections::{HashMap, HashSet},
    error::Error,
};

use anyhow::Result;
use futures_lite::future::Boxed as BoxedFuture;
use iroh::{endpoint::Connecting, protocol::ProtocolHandler};
use serde::{Deserialize, Serialize};
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
    Get(String, oneshot::Sender<String>),
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
}

impl ProtocolHandler for ModelParameterSharing {
    fn accept(&self, connecting: Connecting) -> BoxedFuture<Result<()>> {
        Box::pin(async move { Ok(()) })
    }
}
