use bitvec::vec::BitVec;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use tch::{Device, Kind, TchError, Tensor};

use crate::serializable_kind::SerializableKind;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SerializableTensorData {
    Full(#[serde(with = "serde_bytes")] Vec<u8>),
    OneBit(BitVec),
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

        let num_elements = tensor.numel();

        let data = if tensor.kind() == Kind::Bool {
            // TODO optimization: you can pack (00000000, 00000000, 00000001, 00000000) into 0010 on GPU.
            // bit_weights = [1, 2, 4, 8, 16, 32, 64, 128]
            // multiply and sum along last dimension
            // each group of 8 bits becomes one byte
            // packed = (reshaped * bit_weights).sum(dim=-1)
            let flat_tensor = tensor.flatten(0, -1);

            let data = (0..num_elements)
                .map(|i| flat_tensor.int64_value(&[i as i64]) == 1)
                .collect();
            SerializableTensorData::OneBit(data)
        } else {
            let elt_size = tensor.kind().elt_size_in_bytes();
            let mut data = vec![0u8; num_elements * elt_size];
            tensor.copy_data_u8(&mut data, num_elements);
            SerializableTensorData::Full(data)
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
            SerializableTensorData::OneBit(bits) => {
                let values: Vec<u8> = bits.iter().map(|x| if *x { 1 } else { 0 }).collect();
                Tensor::from_slice(&values)
                    .reshape(&value.dims)
                    .to_kind((&value.kind).into())
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

        truth.print();
        result.print();

        assert!(result.equal(&truth));
    }
}
