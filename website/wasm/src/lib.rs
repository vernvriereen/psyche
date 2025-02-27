use psyche_solana_coordinator::{coordinator_account_from_bytes, ClientId, CoordinatorAccount};
use serde::ser::Serialize;
use ts_rs::TS;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_COORDINATOR_DEF: &str = r#"
import { CoordinatorInstanceState } from "./CoordinatorInstanceState.js";
import { ClientId } from "./ClientId.js";

export type PsycheCoordinator = CoordinatorInstanceState;
"#;

#[wasm_bindgen(unchecked_return_type = "PsycheCoordinator")]
pub fn load_coordinator_from_bytes(bytes: Vec<u8>) -> Result<JsValue, JsError> {
    Ok((coordinator_account_from_bytes(&bytes)?.state.serialize(
        &serde_wasm_bindgen::Serializer::new().serialize_large_number_types_as_bigints(true),
    ))?)
}

#[allow(dead_code)]
#[derive(TS)]
#[ts(export)]
pub struct DummyCoordinatorAccount(CoordinatorAccount);

#[allow(dead_code)]
#[derive(TS)]
#[ts(export)]
pub struct DummyClientId(ClientId);
