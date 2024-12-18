use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub fn serde_serialize_string<S>(
    run_id: &[u8],
    serializer: S,
) -> std::result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    // Convert bytes to string, trimming null bytes
    let s = String::from_utf8_lossy(run_id)
        .trim_matches(char::from(0))
        .to_string();
    serializer.serialize_str(&s)
}

pub fn serde_deserialize_string<'de, D, const N: usize>(
    deserializer: D,
) -> std::result::Result<[u8; N], D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = <std::string::String as Deserialize>::deserialize(deserializer)?;
    let mut bytes = [0u8; N];
    let len = std::cmp::min(s.len(), N);
    bytes[..len].copy_from_slice(&s.as_bytes()[..len]);
    Ok(bytes)
}

pub fn serde_serialize_optional_string<S, const N: usize>(
    run_id: &Option<[u8; N]>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if let Some(run_id) = run_id {
        let s = String::from_utf8_lossy(run_id)
            .trim_matches(char::from(0))
            .to_string();
        serializer.serialize_str(&s)
    } else {
        serializer.serialize_none()
    }
}

pub fn serde_deserialize_optional_string<'de, D, const N: usize>(
    deserializer: D,
) -> std::result::Result<Option<[u8; N]>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    if let Some(s) = s {
        let mut bytes = [0u8; N];
        let len = std::cmp::min(s.len(), N);
        bytes[..len].copy_from_slice(&s.as_bytes()[..len]);
        Ok(Some(bytes))
    } else {
        Ok(None)
    }
}

pub fn serde_serialize_array_as_vec<S, T: Serialize + Clone>(
    array: &[T],
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    array.to_vec().serialize(serializer)
}

pub fn serde_deserialize_vec_to_array<'de, D, T, const N: usize>(
    deserializer: D,
) -> Result<[T; N], D::Error>
where
    D: Deserializer<'de>,
    T: Default + Copy + Deserialize<'de>,
{
    let vec = Vec::<T>::deserialize(deserializer)?;
    let mut arr = [T::default(); N];
    let len = std::cmp::min(vec.len(), N);
    arr[..len].copy_from_slice(&vec[..len]);
    Ok(arr)
}
