use psyche_core::BatchId;
use psyche_modeling::DistroResult;
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt,
    io::{BufReader, Cursor, Read},
    num::TryFromIntError,
};
use tch::{Device, Tensor};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SerializedDistroResult {
    pub sparse_idx: Vec<u8>,
    pub sparse_val: Vec<u8>,
    pub xshape: Vec<u16>,
    pub totalk: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransmittableDistroResult {
    pub step: u32,
    pub batch_id: BatchId,
    pub distro_results: Vec<SerializedDistroResult>,
}

fn serialize_tensor(tensor: &Tensor) -> std::result::Result<Vec<u8>, tch::TchError> {
    let mut buffer = Vec::new();
    tensor.save_to_stream(&mut buffer)?;
    Ok(buffer)
}

#[derive(Debug)]
pub enum SerializeDistroResultError {
    Tch(tch::TchError),
    ShapeInt(TryFromIntError),
}

impl fmt::Display for SerializeDistroResultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SerializeDistroResultError::Tch(err) => write!(f, "Torch error: {}", err),
            SerializeDistroResultError::ShapeInt(err) => {
                write!(f, "Shape had invalid u16: {}", err)
            }
        }
    }
}

impl Error for SerializeDistroResultError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SerializeDistroResultError::Tch(err) => Some(err),
            SerializeDistroResultError::ShapeInt(err) => Some(err),
        }
    }
}

impl From<tch::TchError> for SerializeDistroResultError {
    fn from(err: tch::TchError) -> Self {
        SerializeDistroResultError::Tch(err)
    }
}

impl From<TryFromIntError> for SerializeDistroResultError {
    fn from(err: TryFromIntError) -> Self {
        SerializeDistroResultError::ShapeInt(err)
    }
}

impl TryFrom<&DistroResult> for SerializedDistroResult {
    type Error = SerializeDistroResultError;

    fn try_from(value: &DistroResult) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            sparse_idx: serialize_tensor(&value.sparse_idx)?,
            sparse_val: serialize_tensor(&value.sparse_val)?,
            xshape: value
                .xshape
                .iter()
                .map(|&x| u16::try_from(x))
                .collect::<Result<Vec<u16>, _>>()?,
            totalk: value.totalk,
        })
    }
}

impl TryFrom<&SerializedDistroResult> for DistroResult {
    type Error = tch::TchError;

    fn try_from(value: &SerializedDistroResult) -> std::result::Result<Self, Self::Error> {
        let mut distro_result = Self {
            sparse_idx: Tensor::load_from_stream_with_device(
                Cursor::new(&value.sparse_idx),
                Device::Cpu,
            )?,
            sparse_val: Tensor::load_from_stream_with_device(
                Cursor::new(&value.sparse_val),
                Device::Cpu,
            )?,
            xshape: value.xshape.iter().map(|x| *x as i64).collect(),
            totalk: value.totalk,
            stats: None,
        };
        // don't pin - we can run on cpu if we don't!
        if Device::cuda_if_available().is_cuda() {
            // idx doesn't matter here
            distro_result.sparse_idx = distro_result.sparse_idx.pin_memory(Device::Cuda(0));
            distro_result.sparse_val = distro_result.sparse_val.pin_memory(Device::Cuda(0));
        }
        Ok(distro_result)
    }
}

pub fn distro_results_to_bytes(
    results: &[SerializedDistroResult],
) -> Result<Vec<u8>, postcard::Error> {
    let mut buf = Vec::new();
    for result in results {
        buf.extend(postcard::to_stdvec(result)?);
    }
    Ok(buf)
}

pub fn distro_results_from_reader<R: Read>(reader: R) -> DistroResultIterator<R> {
    DistroResultIterator::new(reader)
}

pub enum DistroResultsReaderError {
    Postcard(postcard::Error),
    Io(std::io::Error),
}

impl Error for DistroResultsReaderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DistroResultsReaderError::Postcard(err) => Some(err),
            DistroResultsReaderError::Io(err) => Some(err),
        }
    }
}

impl fmt::Display for DistroResultsReaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DistroResultsReaderError::Postcard(err) => write!(f, "Postcard error: {}", err),
            DistroResultsReaderError::Io(err) => write!(f, "I/O error: {}", err),
        }
    }
}

impl fmt::Debug for DistroResultsReaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DistroResultsReaderError::Postcard(err) => write!(f, "Postcard({:?})", err),
            DistroResultsReaderError::Io(err) => write!(f, "Io({:?})", err),
        }
    }
}

pub struct DistroResultIterator<R: Read> {
    reader: BufReader<R>,
    buffer: Vec<u8>,
}

impl<R: Read> DistroResultIterator<R> {
    pub fn new(reader: R) -> Self {
        DistroResultIterator {
            reader: BufReader::new(reader),
            buffer: Vec::new(),
        }
    }
}

impl<R: Read> Iterator for DistroResultIterator<R> {
    type Item = Result<SerializedDistroResult, DistroResultsReaderError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match postcard::take_from_bytes::<SerializedDistroResult>(&self.buffer) {
                Ok((result, remaining)) => {
                    self.buffer = remaining.to_vec();
                    return Some(Ok(result));
                }
                Err(postcard::Error::DeserializeUnexpectedEnd) => {
                    // Not enough data, need to read more
                    let mut chunk = [0u8; 1024]; // Adjust chunk size as needed
                    match self.reader.read(&mut chunk) {
                        Ok(0) if self.buffer.is_empty() => return None, // EOF and no partial data
                        Ok(0) => {
                            return Some(Err(DistroResultsReaderError::Postcard(
                                postcard::Error::DeserializeUnexpectedEnd,
                            )))
                        }
                        Ok(n) => self.buffer.extend_from_slice(&chunk[..n]),
                        Err(e) => return Some(Err(DistroResultsReaderError::Io(e))),
                    }
                }
                Err(e) => return Some(Err(DistroResultsReaderError::Postcard(e))),
            }
        }
    }
}
