use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use tch::{Device, Kind, TchError, Tensor};

use crate::serializable_kind::SerializableKind;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SerializableTensorData {
    Full(#[serde(with = "serde_bytes")] Vec<u8>),
    OneBit(#[serde(with = "serde_bytes")] Vec<u8>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SerializableTensor {
    dims: Vec<i64>,
    kind: SerializableKind,
    requires_grad: bool,
    data: SerializableTensorData,
}

impl TryFrom<&Tensor> for SerializableTensor {
    type Error = TchError;

    fn try_from(tensor: &Tensor) -> Result<Self, Self::Error> {
        // tensor must be on cpu & contiguous to read as &[u8]
        let tensor = match (tensor.device(), tensor.is_contiguous()) {
            (Device::Cpu, true) => tensor.shallow_clone(),
            (Device::Cpu, false) => tensor.contiguous(),
            (_, true) => tensor.to_device(Device::Cpu),
            (_, false) => tensor.to_device(Device::Cpu).contiguous(),
        };

        debug_assert!(tensor.is_contiguous());
        debug_assert_eq!(tensor.device(), Device::Cpu);

        let dims = tensor.size();
        let kind = tensor.kind().into();
        let requires_grad = tensor.requires_grad();

        fn tensor_to_bytes(tensor: &Tensor) -> Vec<u8> {
            let num_elements = tensor.numel();
            let elt_size = tensor.kind().elt_size_in_bytes();
            let mut data = vec![0u8; num_elements * elt_size];
            tensor.copy_data_u8(&mut data, num_elements);
            data
        }

        let data = if tensor.kind() == Kind::Bool {
            // this pad and reshape operation is equivalent to taking a tensor of
            // [0, 1, 1, 0, 1, 1, 1, 0, 0, 1, 1, 0, 1, 1, 1, 1]
            // and transforming it into [0b01101110, 0b01101111]
            let n_bits = tensor.numel() as i64;

            // first we pad lengths to multiple of 8, since final array should be &[u8]
            let pad_size = (8 - (n_bits % 8)) % 8;
            let padded = if pad_size > 0 {
                Tensor::f_pad(&tensor, [0, pad_size], "constant", Some(0.0))?
            } else {
                tensor.shallow_clone()
            };

            // then we reshape to (..., N/8, 8)
            let new_shape: Vec<i64> = vec![(n_bits + pad_size) / 8, 8];
            let reshaped = padded.flatten(0, -1).reshape(&new_shape);

            // make a tensor of bit weights (LSB first)
            // which we will multiply with each value consecutively
            // to create packable bits
            let bit_weights = Tensor::from_slice(&[1u8, 2, 4, 8, 16, 32, 64, 128])
                .to_device(tensor.device())
                .to_kind(Kind::Uint8);

            // multiply and sum to pack bits
            let packed = (reshaped.to_kind(Kind::Uint8) * bit_weights).sum_dim_intlist(
                -1,
                false,
                Kind::Uint8,
            );

            SerializableTensorData::OneBit(tensor_to_bytes(&packed))
        } else {
            SerializableTensorData::Full(tensor_to_bytes(&tensor))
        };

        Ok(SerializableTensor {
            dims,
            kind,
            requires_grad,
            data,
        })
    }
}

impl TryFrom<&SerializableTensor> for Tensor {
    type Error = TchError;

    fn try_from(value: &SerializableTensor) -> Result<Self, Self::Error> {
        let tensor = match &value.data {
            SerializableTensorData::Full(data) => {
                Tensor::f_from_data_size(data, &value.dims, (&value.kind).into())?
            }
            SerializableTensorData::OneBit(bytes) => {
                // packed bytes are just a flat 1d slice of bits
                let packed = Tensor::from_slice(bytes).to_kind(Kind::Uint8);

                // make a tensor of bit weights (LSB first) to unpack
                let bit_weights =
                    Tensor::from_slice(&[1u8, 2, 4, 8, 16, 32, 64, 128]).to_kind(Kind::Uint8);

                // calculate total number of elements in the final shape
                let total_elements: i64 = value.dims.iter().product();

                // reshape packed tensor to [..., 1] for broadcasting back to the original shape
                let mut packed_shape = packed.size();
                packed_shape.push(1);
                let reshaped_packed = packed.reshape(&packed_shape);

                // unpack bits by ANDing with the bit weights and convert to boolean
                let bits = reshaped_packed
                    .bitwise_and_tensor(&bit_weights)
                    .to_kind(Kind::Bool);

                // flatten and select only the needed bits
                let flat_bits = bits.flatten(0, -1);
                let needed_bits = flat_bits.slice(0, 0, total_elements, 1);

                // reshape back to original dimensions
                needed_bits.reshape(&value.dims)
            }
        };

        Ok(if value.requires_grad {
            tensor.set_requires_grad(true)
        } else {
            tensor
        })
    }
}

#[cfg(test)]
mod tests {
    use tch::{Device, Kind, Tensor};

    use crate::serializable_tensor::SerializableTensor;

    #[test]
    fn test_roundtrip_tensor1d() {
        let truth = Tensor::from_slice(&[0.6533, 0.2706, -0.2706, -0.6533])
            .to_kind(Kind::Float)
            .to(Device::Cpu);

        let serializable = SerializableTensor::try_from(&truth).unwrap();

        let result = Tensor::try_from(&serializable).unwrap();

        assert!(result.allclose(&truth, 1e-4, 1e-8, false));
    }

    #[test]
    fn test_roundtrip_tensor2d() {
        let truth = Tensor::from_slice2(&[
            [0.6533, 0.2706, -0.2706, -0.6533],
            [230.4230, -25774.5, 0.0, 25.0],
        ])
        .to_kind(Kind::Float)
        .to(Device::Cpu);

        let serializable = SerializableTensor::try_from(&truth).unwrap();

        let result = Tensor::try_from(&serializable).unwrap();

        assert!(result.allclose(&truth, 1e-4, 1e-8, false));
    }

    #[test]
    fn test_roundtrip_bool_tensor1d() {
        let truth = Tensor::from_slice(&[1, 0, 0, 1, 0, 1, 1, 1])
            .to_kind(Kind::Bool)
            .to(Device::Cpu);

        let serializable = SerializableTensor::try_from(&truth).unwrap();

        let result = Tensor::try_from(&serializable).unwrap();

        assert!(result.equal(&truth));
    }

    #[test]
    fn test_roundtrip_bool_tensor2d() {
        let truth = Tensor::from_slice2(&[[1, 0, 0, 1], [0, 1, 1, 1], [1, 0, 1, 0], [1, 1, 0, 1]])
            .to_kind(Kind::Bool)
            .to(Device::Cpu);

        let serializable = SerializableTensor::try_from(&truth).unwrap();
        let result = Tensor::try_from(&serializable).unwrap();

        assert!(result.equal(&truth));
    }
}
