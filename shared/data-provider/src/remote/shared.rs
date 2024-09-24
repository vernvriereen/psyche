use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ServerToClientMessage {
    TrainingData {
        data_ids: Vec<usize>,
        raw_data: Vec<Vec<i32>>,
    },
    RequestRejected {
        data_ids: Vec<usize>,
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
    RequestTrainingData { data_ids: Vec<usize> },
}
