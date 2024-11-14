import torch

import argparse
from typing import Any
import torch
from transformers import AutoModelForCausalLM
from torch.optim import AdamW
import time
import math
from torch.optim.lr_scheduler import LambdaLR

from torch.utils.data import Dataset, DataLoader

import os
import sys
import mmap


def eprint(*args, **kwargs):
    print(*args, file=sys.stderr, **kwargs)


def get_cosine_schedule_with_warmup_and_final_lr(
    optimizer,
    num_warmup_steps,
    num_training_steps,
    warmup_init_lr=0.0,
    final_lr=None,
):
    base_lr = optimizer.param_groups[0]["lr"]
    if final_lr is None:
        final_lr = base_lr * 0.1

    def lr_lambda(current_step: int):
        if current_step < num_warmup_steps:
            # Linear interpolation between warmup_init_lr and base_lr
            return warmup_init_lr + (base_lr - warmup_init_lr) * (
                float(current_step) / float(max(1, num_warmup_steps))
            )

        progress = float(current_step - num_warmup_steps) / float(
            max(1, num_training_steps - num_warmup_steps)
        )
        cosine_decay = 0.5 * (1.0 + math.cos(math.pi * progress))
        return final_lr + (base_lr - final_lr) * cosine_decay

    return LambdaLR(optimizer, lambda x: lr_lambda(x) / base_lr)


class PreTokenizedDataset(Dataset):
    def __init__(self, data_dir: str, sequence_length: int, token_size: int):
        super().__init__()
        self.sequence_length = sequence_length
        self.token_size = token_size

        # Find and open all binary files
        self.mmapped_files = []
        total_size = 0
        for file in os.listdir(data_dir):
            if file.endswith((".bin", ".npy", ".ds")):
                path = os.path.join(data_dir, file)
                with open(path, "rb") as f:
                    mmapped_file = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
                    self.mmapped_files.append(mmapped_file)
                    total_size += os.path.getsize(path)

        if not self.mmapped_files:
            raise ValueError(f"No training data files found in {data_dir}")

        # Calculate all possible sequence positions
        self.sequences = []
        seq_len_bytes = sequence_length * token_size

        for file_idx, mmap_file in enumerate(self.mmapped_files):
            num_sequences = (len(mmap_file) - seq_len_bytes) // token_size
            for seq_start in range(0, num_sequences * token_size, seq_len_bytes):
                self.sequences.append((file_idx, seq_start))

    def __len__(self) -> int:
        return len(self.sequences)

    def __getitem__(self, idx: int) -> torch.Tensor:
        file_idx, byte_offset = self.sequences[idx]
        mmap_file = self.mmapped_files[file_idx]

        # Read sequence bytes
        seq_len_bytes = (self.sequence_length + 1) * self.token_size
        data = mmap_file[byte_offset : byte_offset + seq_len_bytes]

        # Convert bytes to tokens
        tokens = []
        for i in range(0, len(data), self.token_size):
            chunk = data[i : i + self.token_size]
            token = int.from_bytes(chunk, byteorder="little", signed=False)
            tokens.append(token)

        # Convert to tensor
        tokens_tensor = torch.tensor(tokens, dtype=torch.long)
        return (
            tokens_tensor,
            tokens_tensor,
        )  # input and target are the same for causal LM

    def __del__(self):
        # Clean up memory-mapped files
        for f in self.mmapped_files:
            f.close()


def main():
    parser = argparse.ArgumentParser("train-dupe")
    parser.add_argument("--model", type=str, default="emozilla/llama2-215m-init")
    parser.add_argument("--data-path", type=str, default="data")
    parser.add_argument("--sequence-length", type=int, default=2048)
    parser.add_argument("--token-size", type=int, default=2)
    parser.add_argument("--micro-batch", type=int, default=8)
    parser.add_argument("--total-batch", type=int, default=64)
    parser.add_argument("--beta1", type=float, default=0.9)
    parser.add_argument("--beta2", type=float, default=0.95)
    parser.add_argument("--weight-decay", type=float, default=0.1)
    parser.add_argument("--eps", type=float, default=1e-8)
    parser.add_argument("--learning-rate", type=float, default=4e-4)
    parser.add_argument("--warmup-steps", type=int, default=500)
    parser.add_argument("--total-steps", type=int, default=25000)
    parser.add_argument("--max-grad-norm", type=float, default=1.0)
    parser.add_argument("--tensor-parallelism", type=int, required=False)
    parser.add_argument("--optim-stats", action="store_true")
    parser.add_argument("--cpu", action="store_true")
    parser.add_argument("--print-tensors", action="store_true")
    args = parser.parse_args()

    torch.use_deterministic_algorithms(True)
    torch.manual_seed(0)
    train(args)


def train(
    args: Any,
):
    if not args.cpu:
        print("error: only --cpu supported.")
        exit(1)

    if not args.print_tensors:
        print("error: only --print-tensors supported.")
        exit(1)

    print(
        f"starting training run: model {args.model}, data_path {args.data_path}, sequence_length {args.sequence_length}, token_size {args.token_size}, micro_batch {args.micro_batch}, total_batch {args.total_batch}, beta1 {args.beta1:.9f}, beta2 {args.beta2:.9f}, weight_decay {args.weight_decay:.9f}, eps {args.eps:.9f}, learning_rate {args.learning_rate:.9f}, warmup_steps {args.warmup_steps}, total_steps {args.total_steps}, max_grad_norm {args.max_grad_norm:.9f}",
    )

    device = torch.device("cpu")

    model = AutoModelForCausalLM.from_pretrained(args.model, torch_dtype=torch.bfloat16)
    # Setup dataset and dataloader
    dataset = PreTokenizedDataset(args.data_path, args.sequence_length, args.token_size)
    dataloader = DataLoader(
        dataset, batch_size=args.micro_batch, shuffle=False, drop_last=True
    )

    # Setup optimizer
    optimizer = AdamW(
        model.parameters(),
        lr=args.learning_rate,
        betas=(args.beta1, args.beta2),
        eps=args.eps,
        weight_decay=args.weight_decay,
    )

    scheduler = get_cosine_schedule_with_warmup_and_final_lr(
        optimizer, args.warmup_steps, args.total_steps, 0.0, args.learning_rate / 10.0
    )

    # Training loop
    grad_accum_steps = args.total_batch // args.micro_batch

    data = iter(dataloader)

    print("Done loading, starting training.")

    for step in range(args.total_steps):
        start_time = time.time()
        avg_loss = 0.0

        # Gradient accumulation loop
        for i in range(grad_accum_steps):
            eprint(f"py step {step} grad accum {i}")
            batch_inputs, batch_targets = next(data)
            batch_inputs = batch_inputs.to(device)
            batch_targets = batch_targets.to(device)

            # Manual forward pass with logging
            _, seq_length = batch_inputs.size()

            # Get transformer outputs
            transformer_outputs = model.model(
                input_ids=batch_inputs,
                use_cache=False,
            )
            hidden_states = transformer_outputs[0]

            # Get logits through the LM head
            # and convert logits to f32, as megatron & nanotron do.
            logits = model.lm_head(hidden_states).to(torch.float32)

            # Prepare for loss computation
            shift_logits = logits[..., :-1, :].contiguous()
            shift_labels = batch_targets[..., 1:].contiguous()

            shift_logits = shift_logits.view(-1, model.config.vocab_size)
            shift_labels = shift_labels.view(-1)

            loss = torch.nn.functional.cross_entropy(shift_logits, shift_labels)
            loss = loss / grad_accum_steps
            print(f"step {step} grad accum step {i} causal LM forward loss: {tp(loss)}")

            # Backward pass
            loss.backward()

            avg_loss += loss.item() * grad_accum_steps

        # Get trainable variables with their gradients
        variables = []
        for name, param in model.named_parameters():
            if param.requires_grad:  # Only include trainable parameters
                variables.append((name, param))

        # Sort by name
        variables.sort(key=lambda x: x[0])

        # Print gradients
        for name, param in variables:
            if param.grad is not None:
                print(
                    f"step {step} causal LM backward variable: {name} {tp(param.grad.data)}"
                )

        # Clip gradients
        torch.nn.utils.clip_grad_norm_(model.parameters(), args.max_grad_norm)

        # Optimizer step
        optimizer.step()
        scheduler.step()
        optimizer.zero_grad()

        duration = time.time() - start_time

        print(
            f"step: {step}, duration: {duration:.1f}, "
            f"lr: {scheduler.get_last_lr()[0]:.1e}, loss: {avg_loss:.4f}"
        )


def tp(tensor):
    if tensor is None:
        return "None"
    if type(tensor) is tuple:
        vals = "\n".join((tp(i) for i in tensor))
        return vals
    else:
        vals = "\n".join((f"{i:.9f}" for i in tensor.flatten().tolist()))

        # Map PyTorch dtypes to string representations
        dtype_map = {
            "torch.float32": "float32",
            "torch.float64": "float64",
            "torch.float8_e4m3fn": "float8_e4m3fn",
            "torch.float8_e4m3fnuz": "float8_e4m3fnuz",
            "torch.float8_e5m2": "float8_e5m2",
            "torch.float8_e5m2fnuz": "float8_e5m2fnuz",
            "torch.float16": "float16",
            "torch.bfloat16": "bfloat16",
            "torch.uint8": "uint8",
            "torch.uint16": "uint16",
            "torch.uint32": "uint32",
            "torch.uint64": "uint64",
            "torch.int8": "int8",
            "torch.int16": "int16",
            "torch.int32": "int32",
            "torch.int64": "int64",
            "torch.complex32": "complex32",
            "torch.complex64": "complex64",
            "torch.complex128": "complex128",
            "torch.quint8": "quint8",
            "torch.qint8": "qint8",
            "torch.qint32": "qint32",
            "torch.bool": "bool",
        }

        kind = dtype_map.get(str(tensor.dtype), str(tensor.dtype))
        size = ",".join(str(d) for d in tensor.size())

        return f"[ torch.{kind}{{{size}}} ]\n{vals}"


if __name__ == "__main__":
    main()
