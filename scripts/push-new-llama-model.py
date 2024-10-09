from transformers import LlamaConfig, LlamaForCausalLM
from transformers.models.llama.modeling_llama import (
    LlamaMLP,
    LlamaAttention,
    LlamaModel,
)
from torch import nn
import argparse
import torch
import math


def initialize_weights(model: LlamaModel, n_layer: int, n_embd: int) -> None:
    """GPT-NeoX weight initialization (https://arxiv.org/abs/2204.06745)."""
    # Adapted from https://github.com/jzhang38/TinyLlama

    for mod in model.modules():
        if isinstance(mod, (nn.Embedding, nn.Linear)):
            nn.init.normal_(mod.weight, mean=0.0, std=math.sqrt(2.0 / 5 / n_embd))
            if getattr(mod, "bias", None) is not None:
                torch.nn.init.zeros_(mod.bias)

    # need a separate loop because `mod.o_proj` and `mod.down_proj` below are a `nn.Linear` too
    for mod in model.modules():
        if isinstance(mod, LlamaMLP):
            nn.init.normal_(
                mod.down_proj.weight, mean=0.0, std=(1 / math.sqrt(n_embd) / n_layer)
            )
        elif isinstance(mod, LlamaAttention):
            nn.init.normal_(
                mod.o_proj.weight, mean=0.0, std=(1 / math.sqrt(n_embd) / n_layer)
            )


def main(args):
    config = LlamaConfig.from_pretrained(args.config)
    torch.set_default_dtype(args.dtype)
    torch.set_default_device("cuda")
    print("Initializing random model...")
    model = LlamaForCausalLM(config)
    print("GPT-NeoX initialization...")
    initialize_weights(
        model.model, config.num_hidden_layers, config.max_position_embeddings
    )
    print(model)
    total_params = sum(p.numel() for p in model.parameters())
    print(f"Model has {total_params} parameters")
    if not args.dry_run:
        model.push_to_hub(args.repo, private=args.private)


args = argparse.ArgumentParser()
args.add_argument(
    "--config",
    type=str,
    default="TinyLlama/TinyLlama-1.1B-intermediate-step-1431k-3T",
    help="source config repo or path to JSON config",
)
args.add_argument("--repo", type=str, help="destination repo")
args.add_argument("--private", action="store_true", help="push as a private repo")
args.add_argument("--dtype", type=int, default=torch.bfloat16, help="torch dtype")
args.add_argument("--dry-run", action="store_true", help="don't actually push")

main(args.parse_args())