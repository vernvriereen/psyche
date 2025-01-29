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
    str_bytes: &Option<[u8; N]>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if let Some(run_id) = str_bytes {
        let s = String::from_utf8_lossy(run_id)
            .trim_matches(char::from(0))
            .to_string();
        serializer.serialize_some(&s)
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

#[cfg(test)]
mod test {
    use super::*;

    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct MyOptionalStrStruct {
        #[serde(
            serialize_with = "serde_serialize_optional_string",
            deserialize_with = "serde_deserialize_optional_string",
            default
        )]
        field: Option<[u8; 64]>,
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct MyStrStruct {
        #[serde(
            serialize_with = "serde_serialize_string",
            deserialize_with = "serde_deserialize_string"
        )]
        field: [u8; 64],
    }

    #[test]
    fn test_serialize_deserialize_optional_string_some() {
        let my_struct = MyOptionalStrStruct {
            field: Some([1u8; 64]),
        };

        let bytes = postcard::to_stdvec(&my_struct).unwrap();
        let deserialized_struct: MyOptionalStrStruct = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(my_struct, deserialized_struct);
    }

    #[test]
    fn test_serialize_deserialize_optional_string_none() {
        let my_struct = MyOptionalStrStruct { field: None };

        let bytes = postcard::to_stdvec(&my_struct).unwrap();
        let deserialized_struct: MyOptionalStrStruct = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(my_struct, deserialized_struct);
    }

    #[test]
    fn test_serialize_deserialize_string() {
        let my_struct = MyStrStruct { field: [1u8; 64] };

        let bytes = postcard::to_stdvec(&my_struct).unwrap();
        let deserialized_struct: MyStrStruct = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(my_struct, deserialized_struct);
    }
}
