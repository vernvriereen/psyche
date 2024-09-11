use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum ServerToClientMessage {
    Challenge([u8; 32]),
    TrainingData(TrainingData),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TrainingData {
    pub data_id: usize,
    pub raw_data: Vec<i32>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ChallengeResponse(pub Vec<u8>);
