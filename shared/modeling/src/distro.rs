use crate::{
    tensor_parallelism::{tensor_shard, unsharded_tensor_size},
    Communicator,
};
use std::{cmp::Ordering, collections::HashMap, f64::consts::PI, sync::Arc};
use tch::{
    nn::{Optimizer, OptimizerConfig, Sgd, Shard, VarStore},
    Device, Kind, Tensor,
};

#[cfg(feature = "parallelism")]
use tch::ReduceOpType;

#[cfg(feature = "parallelism")]
use crate::tensor_parallelism::unshard_tensor;

struct TransformDCT {
    shape_dict: HashMap<i64, i64>,
    f_dict: HashMap<i64, Tensor>,
    b_dict: HashMap<i64, Tensor>,
}

impl TransformDCT {
    fn new(variables: &[(Tensor, Option<Shard>)], target_chunk: i64) -> Self {
        let _no_grad = tch::no_grad_guard();
        let mut shape_dict = HashMap::new();
        let mut f_dict = HashMap::new();
        let mut b_dict = HashMap::new();

        // Get all variants of model tensor sizes
        // Generate all possible valid DCT sizes for model tensors
        for (variable, shard) in variables {
            let size = match shard {
                Some(shard) => unsharded_tensor_size(&variable.size(), shard),
                None => variable.size(),
            };
            for s in size {
                // Get the closest smallest divisor to the targeted DCT size
                let sc = match shape_dict.get(&s) {
                    Some(sc) => *sc,
                    None => {
                        let sc = Self::get_smaller_split(s, target_chunk);
                        shape_dict.insert(s, sc);
                        sc
                    }
                };

                // Pregenerate DCT basis matrices
                if let std::collections::hash_map::Entry::Vacant(e) = f_dict.entry(sc) {
                    let i = Tensor::eye(sc, (Kind::Float, variable.device()));
                    e.insert(
                        Self::dct(&i, true)
                            .to_kind(variable.kind())
                            .to(variable.device()),
                    );
                    b_dict.insert(
                        sc,
                        Self::idct(&i, true)
                            .to_kind(variable.kind())
                            .to(variable.device()),
                    );
                }
            }
        }
        Self {
            shape_dict,
            f_dict,
            b_dict,
        }
    }

    fn get_prime_divisors(mut n: i64) -> Vec<i64> {
        if n == 0 {
            return Vec::new();
        }
        let mut divisors = Vec::new();
        while n % 2 == 0 {
            divisors.push(2);
            n /= 2;
        }
        while n % 3 == 0 {
            divisors.push(3);
            n /= 3;
        }
        let mut i = 5;
        while i * i <= n {
            for k in [i, i + 2].iter() {
                while n % k == 0 {
                    divisors.push(*k);
                    n /= k;
                }
            }
            i += 6;
        }
        if n > 1 {
            divisors.push(n);
        }
        divisors
    }

    fn get_divisors(n: i64) -> Vec<i64> {
        let mut divisors = Vec::new();
        match n.cmp(&1) {
            Ordering::Equal => {
                divisors.push(1);
            }
            Ordering::Greater => {
                let prime_factors = Self::get_prime_divisors(n);
                divisors = vec![1];
                let mut last_prime = 0;
                let mut factor = 0;
                let mut slice_len = 0;
                // Find all the products that are divisors of n
                for prime in prime_factors {
                    if last_prime != prime {
                        slice_len = divisors.len();
                        factor = prime;
                    } else {
                        factor *= prime;
                    }
                    for i in 0..slice_len {
                        divisors.push(divisors[i] * factor);
                    }
                    last_prime = prime;
                }
                divisors.sort_unstable();
            }
            Ordering::Less => {}
        }
        divisors
    }

    fn get_smaller_split(n: i64, close_to: i64) -> i64 {
        let all_divisors = Self::get_divisors(n);
        for (ix, &val) in all_divisors.iter().enumerate() {
            if val == close_to {
                return val;
            }
            if val > close_to {
                if ix == 0 {
                    return val;
                }
                return all_divisors[ix - 1];
            }
        }
        n
    }

    fn dct_fft_impl(v: &Tensor) -> Tensor {
        v.fft_fft(None, 1, "backward").view_as_real()
    }

    #[allow(unused)]
    fn dct(x: &Tensor, ortho: bool) -> Tensor {
        let x_shape = x.size();
        let n = { *x_shape.last().unwrap() };
        let x = x.contiguous().view([-1, n]);

        let v = Tensor::cat(
            &[x.slice(1, 0, None, 2), x.slice(1, 1, None, 2).flip([1])],
            1,
        );

        let vc = Self::dct_fft_impl(&v);

        let k = -Tensor::arange(n, (Kind::Float, x.device()))
            .unsqueeze(0)
            .g_mul_scalar(PI / (2.0 * n as f64));
        let w_r = k.cos();
        let w_i = k.sin();

        let mut v = vc.select(2, 0) * &w_r - vc.select(2, 1) * &w_i;

        if ortho {
            v.select(1, 0).g_div_scalar_((n as f64).sqrt() * 2.0);
            v.slice(1, 1, None, 1)
                .g_div_scalar_((n as f64 / 2.0).sqrt() * 2.0);
        }

        v.g_mul_scalar_(2.0).view(x_shape.as_slice())
    }

    fn idct_irfft_impl(v: &Tensor) -> Tensor {
        let complex_v = v.view_as_complex();
        let n = v.size()[1];
        complex_v.fft_irfft(Some(n), 1, "backward")
    }

    #[allow(unused)]
    fn idct(x: &Tensor, ortho: bool) -> Tensor {
        let x_shape = x.size();
        let n = { *x_shape.last().unwrap() };

        let mut x_v = x.contiguous().view([-1, n]).f_div_scalar(2.0).unwrap();

        if ortho {
            x_v.slice(1, 0, 1, 1)
                .f_mul_scalar_((n as f64).sqrt() * 2.0)
                .unwrap();
            x_v.slice(1, 1, n, 1)
                .f_mul_scalar_((n as f64 / 2.0).sqrt() * 2.0)
                .unwrap();
        }

        let k = Tensor::arange(n, (Kind::Float, x.device()))
            .f_mul_scalar(PI / (2.0 * n as f64))
            .unwrap()
            .unsqueeze(0);

        let w_r = k.cos();
        let w_i = k.sin();

        let v_t_r = &x_v;
        let v_t_i = Tensor::cat(
            &[
                x_v.slice(1, 0, 1, 1).f_mul_scalar(0.0).unwrap(),
                x_v.flip([1]).slice(1, 0, n - 1, 1).f_neg().unwrap(),
            ],
            1,
        );

        let v_r = v_t_r.f_mul(&w_r).unwrap() - v_t_i.f_mul(&w_i).unwrap();
        let v_i = v_t_r.f_mul(&w_i).unwrap() + v_t_i.f_mul(&w_r).unwrap();

        let v = Tensor::cat(&[v_r.unsqueeze(2), v_i.unsqueeze(2)], 2);

        let v = Self::idct_irfft_impl(&v);

        let mut x = Tensor::zeros(v.size(), (Kind::Float, v.device()));

        x.slice(1, 0, n, 2)
            .f_add_(&v.slice(1, 0, n - (n / 2), 1))
            .unwrap();
        x.slice(1, 1, n, 2)
            .f_add_(&v.flip([1]).slice(1, 0, n / 2, 1))
            .unwrap();

        x.view(x_shape.as_slice())
    }

    fn einsum_2d(x: &Tensor, b: &Tensor, d: Option<&Tensor>) -> Tensor {
        let _no_grad = tch::no_grad_guard();
        match d {
            None => Tensor::einsum("...ij, jb -> ...ib", &[x, b], None::<i64>),
            Some(d_tensor) => {
                // Note: b-c axis output is transposed to chunk DCT in 2D
                Tensor::einsum("...ijkl, jb, ld -> ...ikbd", &[x, b, d_tensor], None::<i64>)
            }
        }
    }

    fn einsum_2d_t(x: &Tensor, b: &Tensor, d: Option<&Tensor>) -> Tensor {
        let _no_grad = tch::no_grad_guard();
        match d {
            None => Tensor::einsum("...ij, jb -> ...ib", &[x, b], None::<i64>),
            Some(d_tensor) => {
                // Note: b-c axis output is transposed to chunk DCT in 2D
                Tensor::einsum("...ijkl, kb, ld -> ...ibjd", &[x, b, d_tensor], None::<i64>)
            }
        }
    }

    fn encode(&mut self, x: &Tensor) -> Tensor {
        let _no_grad = tch::no_grad_guard();
        if x.size().len() > 1 {
            // 2D weights
            let n1 = *self.shape_dict.get(&x.size()[0]).unwrap();
            let n2 = *self.shape_dict.get(&x.size()[1]).unwrap();
            let n1w = self.f_dict.get(&n1).unwrap().to_device(x.device());
            let n2w = self.f_dict.get(&n2).unwrap().to_device(x.device());
            self.f_dict.insert(n1, n1w.copy());
            self.f_dict.insert(n2, n2w.copy());

            // Equivalent to rearrange(x, "(y h) (x w) -> y h x w", h=n1, w=n2)
            let x = x.view([x.size()[0] / n1, n1, x.size()[1] / n2, n2]);
            Self::einsum_2d(&x, &n1w, Some(&n2w))
        } else {
            // 1D weights
            let n1 = *self.shape_dict.get(&x.size()[0]).unwrap();
            let n1w = self.f_dict.get(&n1).unwrap().to_device(x.device());
            self.f_dict.insert(n1, n1w.copy());

            // Equivalent to rearrange(x, "(x w) -> x w", w=n1)
            let x = x.view([-1, n1]);
            Self::einsum_2d(&x, &n1w, None)
        }
    }

    fn decode(&mut self, x: &Tensor) -> Tensor {
        let _no_grad = tch::no_grad_guard();
        let x_shape = x.size();

        if x_shape.len() > 2 {
            // 2D weights
            let n1 = x_shape[2];
            let n2 = x_shape[3];
            let device = x.device();

            let n1w = self.b_dict.get(&n1).unwrap().to_device(device);
            let n2w = self.b_dict.get(&n2).unwrap().to_device(device);

            self.b_dict.insert(n1, n1w.copy());
            self.b_dict.insert(n2, n2w.copy());

            let x = Self::einsum_2d_t(x, &n1w, Some(&n2w));
            let x_shape = x.size();

            // Equivalent to rearrange(x, "y h x w -> (y h) (x w)")
            let (y, h, x_, w) = (x_shape[0], x_shape[1], x_shape[2], x_shape[3]);
            x.reshape([y * h, x_ * w])
        } else {
            // 1D weights
            let n1 = x_shape[1];
            let device = x.device();

            let n1w = self.b_dict.get(&n1).unwrap().to_device(device);
            self.b_dict.insert(n1, n1w.copy());

            let x = Self::einsum_2d_t(x, &n1w, None);
            let x_shape = x.size();

            // Equivalent to rearrange(x, "x w -> (x w)")
            let (x_, w) = (x_shape[0], x_shape[1]);
            x.reshape([x_ * w])
        }
    }
}

pub struct CompressDCT;

impl CompressDCT {
    fn clamp_topk(x: &Tensor, topk: i64) -> i64 {
        let last_dim = x.size()[x.dim() - 1];

        if topk > last_dim {
            last_dim
        } else if topk < 1 {
            1
        } else {
            topk
        }
    }

    pub fn compress(x: &Tensor, topk: i64, quantization: bool) -> (Tensor, Tensor, Vec<i64>, i64) {
        let _no_grad = tch::no_grad_guard();
        let xshape = x.size();
        let x = if xshape.len() > 2 {
            // Equivalent to rearrange(x, "y x h w -> y x (h w)")
            let y = xshape[0];
            let x_dim = xshape[1];
            let h = xshape[2];
            let w = xshape[3];
            x.view([y, x_dim, h * w])
        } else {
            x.shallow_clone()
        };

        let mut totalk = *x.size().last().unwrap();
        let topk = Self::clamp_topk(&x, topk);

        let mut idx = x.abs().topk(topk, -1, true, false).1;
        let mut val = x.gather(-1, &idx, false);

        if totalk <= 256 {
            idx = idx.to_kind(Kind::Uint8);
        } else if totalk <= 65536 {
            idx = idx.to_kind(Kind::UInt16).view_dtype(Kind::Uint8);
        } else if totalk <= 4294967296 {
            idx = idx.to_kind(Kind::UInt32).view_dtype(Kind::Uint8);
        }

        if quantization {
            val = val
                .multiply_scalar(1e4)
                .clamp_(-448., 448.) //min max representable values in float8e4m3fn
                .to_kind(Kind::Float8e4m3fn)
                .view_dtype(Kind::Uint8);
            totalk = -totalk; // indicator we're quantizaed
        }

        (idx, val, xshape, totalk)
    }

    #[allow(unused)]
    pub fn decompress(
        idx: &Tensor,
        val: &Tensor,
        xshape: &[i64],
        totalk: i64,
        kind: Kind,
        device: Device,
    ) -> Tensor {
        let quantized = totalk < 0;
        let totalk = totalk.abs();

        let idx = if totalk <= 256 {
            idx.view_dtype(Kind::Uint8)
        } else if totalk <= 65536 {
            idx.view_dtype(Kind::UInt16)
        } else if totalk <= 4294967296 {
            idx.view_dtype(Kind::UInt32)
        } else {
            idx.shallow_clone()
        }
        .to_kind(Kind::Int64);

        let val = match quantized {
            true => val
                .view_dtype(Kind::Float8e4m3fn)
                .to_kind(kind)
                .multiply_scalar(1e-4),
            false => val.shallow_clone(),
        };

        let mut x: Tensor = Tensor::zeros(xshape, (kind, device));

        if xshape.len() > 2 {
            // 2D weights
            // Equivalent to rearrange(x, "y x h w -> y x (h w)")
            let y = xshape[0];
            let x_dim = xshape[1];
            let h = xshape[2];
            let w = xshape[3];
            x = x.view([y, x_dim, h * w]);
        }

        x.internal_scatter_reduce_(-1, &idx, &val, "mean", false);

        x = x.reshape(xshape);

        if x.size().len() > 2 {
            // 2D weights
            // Equivalent to rearrange(x, "y x (h w) -> y x h w", h=xshape[2])
            let y = xshape[0];
            let x_dim = xshape[1];
            let h = xshape[2];
            let w = xshape[3];
            x = x.view([y, x_dim, h, w]);
        }

        x
    }

    pub fn batch_decompress(
        idx: &[Tensor],
        val: &[Tensor],
        xshape: &[i64],
        totalk: i64,
        kind: Kind,
        device: Device,
    ) -> Tensor {
        let idx_concat = Tensor::cat(idx, -1).to_device(device);
        let val_concat = Tensor::cat(val, -1).to_device(device);
        // Call the decompress method
        Self::decompress(&idx_concat, &val_concat, xshape, totalk, kind, device)
    }
}

struct State {
    delta: Tensor,
}

#[derive(Debug)]
pub struct DistroResult {
    pub sparse_idx: Tensor,
    pub sparse_val: Tensor,
    pub xshape: Vec<i64>,
    pub totalk: i64,
    pub stats: Option<HashMap<String, f64>>,
}

impl Clone for DistroResult {
    fn clone(&self) -> Self {
        Self {
            sparse_idx: self.sparse_idx.shallow_clone(),
            sparse_val: self.sparse_val.shallow_clone(),
            xshape: self.xshape.clone(),
            totalk: self.totalk,
            stats: self.stats.clone(),
        }
    }
}

pub struct Distro {
    sgd: Optimizer,
    compression_decay: f64,
    weight_decay: f64,
    state: Vec<State>,
    transform: TransformDCT,
    comm: Option<Arc<Communicator>>,
    index_to_name: HashMap<usize, Option<String>>,
}

impl Distro {
    pub fn new(
        vs: &VarStore,
        compression_decay: f64,
        compression_chunk: i64,
        weight_decay: f64,
        comm: Option<Arc<Communicator>>,
    ) -> Self {
        let _no_grad = tch::no_grad_guard();
        let mut sgd: Optimizer = Sgd {
            momentum: 0.0,
            dampening: 0.0,
            wd: 0.0,
            nesterov: false,
        }
        .build(vs, 0.1)
        .unwrap();
        sgd.zero_grad_with_set_to_none(false);

        let named_variables = vs.variables().into_iter().collect::<Vec<_>>();
        let variables = sgd.trainable_variables_with_sharding();
        let mut state = Vec::with_capacity(variables.len());
        let mut index_to_name = HashMap::new();

        for (index, (variable, _)) in variables.iter().enumerate() {
            state.push(State {
                delta: variable.zeros_like(),
            });
            index_to_name.insert(
                index,
                named_variables
                    .iter()
                    .find(|x| x.1.is_set_to(variable))
                    .map(|x| x.0.clone()),
            );
        }

        let transform = TransformDCT::new(&variables, compression_chunk);

        Self {
            sgd,
            compression_decay,
            weight_decay,
            state,
            transform,
            comm,
            index_to_name,
        }
    }

    pub fn generate(
        &mut self,
        lr: f64,
        warmup_factor: f64,
        compression_topk: i64,
        quantization: bool,
        stats: bool,
    ) -> Vec<DistroResult> {
        let _no_grad = tch::no_grad_guard();
        let variables = &mut self.sgd.trainable_variables_with_sharding();
        let mut ret = Vec::with_capacity(variables.len());
        for (index, (variable, shard)) in variables.iter_mut().enumerate() {
            let grad_energy: Option<f64> = match stats {
                true => Some(
                    variable
                        .grad()
                        .norm_scalaropt_dtype(1, Kind::Float)
                        .try_into()
                        .unwrap(),
                ),
                _ => None,
            };

            // weight decay
            if self.weight_decay != 0.0 {
                let _t = variable.g_mul_scalar_(1.0 - lr * self.weight_decay);
            }

            // decay delta
            let mut compression_decay = self.compression_decay;
            if warmup_factor < 1.0 {
                let momentum_factor = warmup_factor.powf(0.1) * 0.1 + 0.9;
                compression_decay *= momentum_factor;
            }
            let delta = &mut self.state.get_mut(index).unwrap().delta;
            if compression_decay != 1.0 {
                let _t = delta.g_mul_scalar_(compression_decay);
            }

            // add delta to new gradient
            let _ = delta.g_add_(&variable.grad().multiply_scalar(lr));

            let (sparse_idx, sparse_val, xshape, totalk, transmit_grad, full_delta) = match shard {
                #[cfg(feature = "parallelism")]
                Some(shard) => {
                    assert!(self.comm.is_some());
                    let comm = self.comm.as_ref().unwrap();

                    // gather delta
                    let shards = (0..shard.world_size)
                        .map(|_| delta.empty_like())
                        .collect::<Vec<_>>();
                    comm.all_gather(&shards, &delta).unwrap();
                    let gathered_delta = unshard_tensor(shards, shard);

                    // Compress delta
                    let (sparse_idx, sparse_val, xshape, totalk) = CompressDCT::compress(
                        &self.transform.encode(&gathered_delta),
                        compression_topk,
                        quantization,
                    );

                    // Estimate transmitted delta
                    let transmit_grad = self.transform.decode(&CompressDCT::decompress(
                        &sparse_idx,
                        &sparse_val,
                        &xshape,
                        totalk,
                        variable.kind(),
                        variable.device(),
                    ));
                    let transmit_grad = tensor_shard(&transmit_grad, shard);

                    (
                        sparse_idx,
                        sparse_val,
                        xshape,
                        totalk,
                        transmit_grad,
                        gathered_delta,
                    )
                }
                #[cfg(not(feature = "parallelism"))]
                Some(_) => panic!("Sharded tensor without parallelism feature?"),
                None => {
                    // Compress delta
                    let (sparse_idx, sparse_val, xshape, totalk) = CompressDCT::compress(
                        &self.transform.encode(delta),
                        compression_topk,
                        quantization,
                    );

                    // Estimate transmitted delta
                    let transmit_grad = self.transform.decode(&CompressDCT::decompress(
                        &sparse_idx,
                        &sparse_val,
                        &xshape,
                        totalk,
                        variable.kind(),
                        variable.device(),
                    ));

                    (
                        sparse_idx,
                        sparse_val,
                        xshape,
                        totalk,
                        transmit_grad,
                        delta.shallow_clone(),
                    )
                }
            };

            let delta_transmit_energies: Option<(f64, f64)> = match stats {
                true => Some((
                    full_delta
                        .norm_scalaropt_dtype(1, Kind::Float)
                        .try_into()
                        .unwrap(),
                    transmit_grad
                        .norm_scalaropt_dtype(1, Kind::Float)
                        .try_into()
                        .unwrap(),
                )),
                false => None,
            };

            // Remove transmitted from delta
            let _t = delta.g_sub_(&transmit_grad);

            let sparse_val = sparse_val.sign();

            ret.push(DistroResult {
                sparse_idx,
                sparse_val,
                xshape,
                totalk,
                stats: match stats {
                    true => match self.index_to_name.get(&index) {
                        Some(Some(name)) => Some(HashMap::from([
                            (
                                format!("{name}.delta_energy"),
                                delta_transmit_energies.map(|x| x.0).unwrap(),
                            ),
                            (
                                format!("{name}.transmit_energy"),
                                delta_transmit_energies.map(|x| x.1).unwrap(),
                            ),
                            (format!("{name}.grad_energy"), grad_energy.unwrap()),
                        ])),
                        _ => None,
                    },
                    false => None,
                },
            });
        }
        ret
    }

    #[allow(unused)]
    pub fn apply(&mut self, results: &[Vec<DistroResult>], lr: f64) {
        let _no_grad = tch::no_grad_guard();
        if results.is_empty() {
            return;
        }
        let mut trainable_variables_with_sharding = self.sgd.trainable_variables_with_sharding();
        for result in results {
            assert!(result.len() == trainable_variables_with_sharding.len());
        }
        for (index, (variable, shard)) in trainable_variables_with_sharding.iter_mut().enumerate() {
            let device = variable.device();

            let indicies = results
                .iter()
                .map(|x| x[index].sparse_idx.to_device(device))
                .collect::<Vec<_>>();
            let values = results
                .iter()
                .map(|x| x[index].sparse_val.to_device(device))
                .collect::<Vec<_>>();

            // Decode grad from all nodes
            let decompressed = CompressDCT::batch_decompress(
                &indicies,
                &values,
                &results[0][index].xshape,
                results[0][index].totalk,
                variable.kind(),
                device,
            );

            let new_grad = self.transform.decode(&decompressed);

            // Set grad to values
            variable.grad().copy_(&match shard {
                Some(shard) => tensor_shard(&new_grad, shard),
                None => new_grad,
            });

            // Sign-SGD
            variable.grad().sign_();
        }
        // SGD step
        self.sgd.set_lr(lr);
        self.sgd.step();
        self.zero_grad();
    }

    pub fn zero_grad(&mut self) {
        self.sgd.zero_grad_with_set_to_none(false);
    }

    pub fn trainable_variables(&self) -> Vec<Tensor> {
        self.sgd.trainable_variables()
    }

    pub fn trainable_variables_with_sharding(&self) -> Vec<(Tensor, Option<Shard>)> {
        self.sgd.trainable_variables_with_sharding()
    }

    /// Clips gradient norm, properly handling tensor-parallel parameters.
    ///
    /// For a model with both sharded and replicated parameters, the true L2 norm is:
    /// sqrt(||w_shared||^2 + ||w_replicated||^2) where:
    /// - w_shared are parameters sharded across ranks (like TP linear layers)
    /// - w_replicated are parameters replicated on all ranks (like layernorms)
    ///
    /// For sharded parameters, since each rank has an orthogonal slice of the full parameter:
    /// ||w_shared||^2 = ||w_shared_1||^2 + ||w_shared_2||^2 + ... + ||w_shared_n||^2
    /// where w_shared_i is the shard on rank i. We compute this via all_reduce_sum of local squared norms.
    ///
    /// For replicated parameters:
    /// ||w_replicated||^2 is identical on all ranks, so we compute it locally.
    ///
    /// The orthogonality of sharded parameters across ranks ensures that:
    /// total_norm = sqrt(all_reduce(||w_shared_local||^2) + ||w_replicated||^2)
    /// gives us the correct global L2 norm as if all parameters were on a single device.
    pub fn clip_grad_norm(&mut self, max_norm: f64) {
        let vars = self.sgd.trainable_variables_with_sharding();
        let device = if !vars.is_empty() {
            vars[0].0.device()
        } else {
            return;
        };

        let mut sharded_norm_sq = Tensor::zeros([], (Kind::Float, device));
        let mut replicated_norm_sq = Tensor::zeros([], (Kind::Float, device));

        for (param, shard) in &self.sgd.trainable_variables_with_sharding() {
            let grad = param.grad();
            if grad.defined() {
                let local_norm = grad.norm();
                let local_norm_sq = &local_norm * &local_norm;

                match shard {
                    Some(_) => sharded_norm_sq += local_norm_sq,
                    None => replicated_norm_sq += local_norm_sq,
                }
            }
        }
        #[cfg(feature = "parallelism")]
        if let Some(comm) = &self.comm {
            comm.all_reduce(&[&sharded_norm_sq], ReduceOpType::Sum)
                .unwrap();
        }
        #[cfg(not(feature = "parallelism"))]
        if self.comm.is_some() {
            panic!("communicator passed, but parallelism is not enabled.");
        }

        let total_norm: f64 = (sharded_norm_sq + replicated_norm_sq)
            .sqrt()
            .try_into()
            .unwrap();

        if total_norm > max_norm {
            let scale = max_norm / (total_norm + 1e-6);
            for (param, _) in vars {
                let mut grad = param.grad();
                if grad.defined() {
                    let _t = grad.g_mul_scalar_(scale);
                }
            }
        }
    }
}

unsafe impl Send for Distro {}

#[cfg(test)]
mod tests {

    use tch::Device;

    use super::*;

    #[test]
    fn test_get_prime_divisors() {
        assert_eq!(TransformDCT::get_prime_divisors(1), Vec::<i64>::new());
        assert_eq!(TransformDCT::get_prime_divisors(2), vec![2]);
        assert_eq!(TransformDCT::get_prime_divisors(12), vec![2, 2, 3]);
        assert_eq!(TransformDCT::get_prime_divisors(15), vec![3, 5]);
        assert_eq!(TransformDCT::get_prime_divisors(100), vec![2, 2, 5, 5]);
        assert_eq!(TransformDCT::get_prime_divisors(2310), vec![2, 3, 5, 7, 11]);
    }

    #[test]
    fn test_get_divisors() {
        assert_eq!(TransformDCT::get_divisors(1), vec![1]);
        assert_eq!(TransformDCT::get_divisors(2), vec![1, 2]);
        assert_eq!(TransformDCT::get_divisors(12), vec![1, 2, 3, 4, 6, 12]);
        assert_eq!(TransformDCT::get_divisors(15), vec![1, 3, 5, 15]);
        assert_eq!(
            TransformDCT::get_divisors(100),
            vec![1, 2, 4, 5, 10, 20, 25, 50, 100]
        );
    }

    #[test]
    fn test_get_smaller_split() {
        assert_eq!(TransformDCT::get_smaller_split(12, 3), 3);
        assert_eq!(TransformDCT::get_smaller_split(12, 4), 4);
        assert_eq!(TransformDCT::get_smaller_split(12, 5), 4);
        assert_eq!(TransformDCT::get_smaller_split(100, 7), 5);
        assert_eq!(TransformDCT::get_smaller_split(100, 26), 25);
        assert_eq!(TransformDCT::get_smaller_split(100, 101), 100);
        assert_eq!(TransformDCT::get_smaller_split(1, 1), 1);
    }

    #[test]
    fn test_edge_cases() {
        assert_eq!(TransformDCT::get_prime_divisors(0), Vec::<i64>::new());
        assert_eq!(TransformDCT::get_divisors(0), Vec::<i64>::new());
        assert_eq!(TransformDCT::get_smaller_split(0, 1), 0);
    }

    #[test]
    fn test_large_numbers() {
        assert_eq!(
            TransformDCT::get_prime_divisors(1000000007),
            vec![1000000007]
        ); // Large prime
        assert_eq!(TransformDCT::get_divisors(1000000007), vec![1, 1000000007]);
        assert_eq!(TransformDCT::get_smaller_split(1000000007, 500000000), 1);
    }

    #[test]
    fn test_dct() {
        let eye = Tensor::eye(4, (Kind::Float, Device::Cpu));
        let truth = _2d_float(&[
            [0.5000, 0.6533, 0.5000, 0.2706],
            [0.5000, 0.2706, -0.5000, -0.6533],
            [0.5000, -0.2706, -0.5000, 0.6533],
            [0.5000, -0.6533, 0.5000, -0.2706],
        ]);
        let result = TransformDCT::dct(&eye, true);
        assert!(result.allclose(&truth, 1e-4, 1e-8, false));
    }

    fn _2d_float<T: AsRef<[f64]>>(x: &[T]) -> Tensor {
        Tensor::from_slice2(x).to_kind(Kind::Float).to(Device::Cpu)
    }

    fn _2d_int<T: AsRef<[i64]>>(x: &[T]) -> Tensor {
        Tensor::from_slice2(x).to_kind(Kind::Int64).to(Device::Cpu)
    }

    fn _1d_float(x: &[f64]) -> Tensor {
        Tensor::from_slice(x).to_kind(Kind::Float).to(Device::Cpu)
    }

    fn _1d_int(x: &[i64]) -> Tensor {
        Tensor::from_slice(x).to_kind(Kind::Int64).to(Device::Cpu)
    }

    #[test]
    fn test_idct() {
        let eye = Tensor::eye(4, (Kind::Float, Device::Cpu));
        let truth = _2d_float(&[
            [0.5000, 0.5000, 0.5000, 0.5000],
            [0.6533, 0.2706, -0.2706, -0.6533],
            [0.5000, -0.5000, -0.5000, 0.5000],
            [0.2706, -0.6533, 0.6533, -0.2706],
        ]);
        let result = TransformDCT::idct(&eye, true);
        assert!(result.allclose(&truth, 1e-4, 1e-8, false));
    }

    #[test]
    fn test_compress_2d() {
        let r = _2d_float(&[
            [0.1911, 0.4076, 0.1649, 0.8059],
            [0.2803, 0.9381, 0.9071, 0.2573],
            [0.4070, 0.5765, 0.7226, 0.9486],
            [0.0737, 0.7378, 0.1898, 0.2990],
        ]);
        let truth = (
            _2d_int(&[[3, 1], [1, 2], [3, 2], [1, 3]]),
            _2d_float(&[
                [0.8059, 0.4076],
                [0.9381, 0.9071],
                [0.9486, 0.7226],
                [0.7378, 0.2990],
            ]),
            vec![4i64, 4i64],
            4i64,
        );
        let ret = CompressDCT::compress(&r, 2, false);
        assert_eq!(truth.0, ret.0);
        assert!(truth.1.allclose(&ret.1, 1e-4, 1e-8, false));
        assert_eq!(truth.2, ret.2);
        assert_eq!(4, ret.3);
    }

    #[test]
    fn test_compress_1d() {
        let r = _1d_float(&[
            0.5223, 0.9625, 0.5487, 0.2152, 0.2161, 0.0363, 0.4944, 0.0974,
        ]);
        let truth = (
            _1d_int(&[1, 2]),
            _1d_float(&[0.9625, 0.5487]),
            vec![8i64],
            8i64,
        );
        let ret = CompressDCT::compress(&r, 2, false);
        assert_eq!(truth.0, ret.0);
        assert!(truth.1.allclose(&ret.1, 1e-4, 1e-8, false));
        assert_eq!(truth.2, ret.2);
        assert_eq!(8, ret.3);
    }

    #[test]
    fn test_decompress_1d() {
        let p = _1d_float(&[0.0]);
        let idx = _1d_int(&[1, 2]);
        let val = _1d_float(&[0.9625, 0.5487]);
        let xshape = vec![8i64];
        let truth = _1d_float(&[
            0.0000, 0.9625, 0.5487, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000,
        ]);
        let ret = CompressDCT::decompress(&idx, &val, &xshape, i64::MAX, p.kind(), p.device());
        assert!(truth.allclose(&ret, 1e-4, 1e-8, false));
    }

    #[test]
    fn test_decompress_2d() {
        let p = _1d_float(&[0.0]);
        let idx = _2d_int(&[[0, 2], [1, 2], [2, 3], [3, 1]]);
        let val = _2d_float(&[
            [0.8988, 0.5175],
            [0.9882, 0.8945],
            [0.8285, 0.8163],
            [0.9093, 0.7600],
        ]);
        let xshape = vec![4i64, 4i64];
        let truth = _2d_float(&[
            [0.8988, 0.0000, 0.5175, 0.0000],
            [0.0000, 0.9882, 0.8945, 0.0000],
            [0.0000, 0.0000, 0.8285, 0.8163],
            [0.0000, 0.7600, 0.0000, 0.9093],
        ]);
        let ret = CompressDCT::decompress(&idx, &val, &xshape, i64::MAX, p.kind(), p.device());
        assert!(truth.allclose(&ret, 1e-4, 1e-8, false));
    }

    #[test]
    fn test_encode_1d() {
        let a = Tensor::arange(8, (Kind::Float, Device::Cpu));
        let truth = _1d_float(&[
            9.8995e+00,
            -6.4423e+00,
            -4.7684e-07,
            -6.7345e-01,
            2.3842e-07,
            -2.0090e-01,
            -1.1921e-07,
            -5.0702e-02,
        ]);
        let ret = TransformDCT::new(&[(a.copy(), None)], 64)
            .encode(&a)
            .squeeze();
        assert!(truth.allclose(&ret, 1e-4, 1e-8, false));
    }

    #[test]
    fn test_encode_2d() {
        let b = Tensor::eye(4, (Kind::Float, Device::Cpu));
        let truth = _2d_float(&[
            [1.0000e+00, 0.0000e+00, 0.0000e+00, 0.0000e+00],
            [0.0000e+00, 1.0000e+00, 0.0000e+00, -5.9605e-08],
            [0.0000e+00, 0.0000e+00, 1.0000e+00, 0.0000e+00],
            [0.0000e+00, -5.9605e-08, 0.0000e+00, 1.0000e+00],
        ]);
        let ret = TransformDCT::new(&[(b.copy(), None)], 64)
            .encode(&b)
            .squeeze();
        assert!(truth.allclose(&ret, 1e-4, 1e-8, false));
    }

    #[test]
    fn test_decode_1d() {
        let a = Tensor::arange(8, (Kind::Float, Device::Cpu));
        let a_ = _2d_float(&[[
            9.8995e+00,
            -6.4423e+00,
            -4.7684e-07,
            -6.7345e-01,
            2.3842e-07,
            -2.0090e-01,
            -1.1921e-07,
            -5.0702e-02,
        ]]);
        let truth = _1d_float(&[
            -2.2352e-07,
            1.0000e+00,
            2.0000e+00,
            3.0000e+00,
            4.0000e+00,
            5.0000e+00,
            6.0000e+00,
            7.0000e+00,
        ]);
        let ret = TransformDCT::new(&[(a, None)], 64).decode(&a_);
        assert!(truth.allclose(&ret, 1e-4, 1e-4, false));
    }

    #[test]
    fn test_decode_2d() {
        let b = Tensor::eye(4, (Kind::Float, Device::Cpu));
        let b_ = _2d_float(&[
            [1.0000e+00, 0.0000e+00, 0.0000e+00, 0.0000e+00],
            [0.0000e+00, 1.0000e+00, 0.0000e+00, -5.9605e-08],
            [0.0000e+00, 0.0000e+00, 1.0000e+00, 0.0000e+00],
            [0.0000e+00, -5.9605e-08, 0.0000e+00, 1.0000e+00],
        ])
        .unsqueeze(0)
        .unsqueeze(0);
        let truth = _2d_float(&[
            [1.0000e+00, 1.4901e-08, 4.4703e-08, 4.4703e-08],
            [2.9802e-08, 1.0000e+00, -2.9802e-08, 4.4703e-08],
            [4.4703e-08, -2.9802e-08, 1.0000e+00, 2.9802e-08],
            [4.4703e-08, 4.4703e-08, 1.4901e-08, 1.0000e+00],
        ]);
        let ret = TransformDCT::new(&[(b, None)], 64).decode(&b_);
        assert!(truth.allclose(&ret, 1e-4, 1e-4, false));
    }
}
