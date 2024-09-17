use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ServerToClientMessage {
    TrainingData {
        data_id: usize,
        raw_data: Vec<i32>,
    },
    RequestRejected {
        data_id: usize,
        reason: RejectionReason,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum RejectionReason {
    NotInThisRound,
    WrongDataIdForStep,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ClientToServerMessage {
    RequestTrainingData { data_id: usize },
}
