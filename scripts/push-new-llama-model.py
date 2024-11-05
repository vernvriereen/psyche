from transformers import LlamaConfig, LlamaForCausalLM, AutoTokenizer
from transformers.models.llama.modeling_llama import (
    LlamaMLP,
    LlamaAttention,
    LlamaModel,
)
from torch import nn
import argparse
import torch
import math


def _init_normal(module, std: float, cutoff_factor: float = 3.0):
    with torch.no_grad():
        cutoff = std * cutoff_factor
        weight = module.weight
        weight.normal_(0, std)
        torch.clamp_(weight, min=-cutoff, max=cutoff)
        if hasattr(module, "bias") and module.bias is not None:
            module.bias.zero_()


def initialize_weights(model: LlamaForCausalLM):
    """Initialize model weights using the "Mitchell" initialization scheme"""

    wte_std = 1 / math.sqrt(model.config.hidden_size)
    _init_normal(model.model.embed_tokens, std=wte_std)

    for layer_id, layer in enumerate(model.model.layers):
        attn_std = 1 / math.sqrt(model.config.hidden_size)
        _init_normal(layer.self_attn.q_proj, std=attn_std)
        _init_normal(layer.self_attn.k_proj, std=attn_std)
        _init_normal(layer.self_attn.v_proj, std=attn_std)

        attn_out_std = 1 / (math.sqrt(2 * model.config.hidden_size * (layer_id + 1)))
        _init_normal(layer.self_attn.o_proj, std=attn_out_std)

        ff_std = 1 / math.sqrt(model.config.hidden_size)
        _init_normal(layer.mlp.gate_proj, std=ff_std)
        _init_normal(layer.mlp.up_proj, std=ff_std)

        ff_out_std = 1 / (
            math.sqrt(2 * layer.mlp.down_proj.in_features * (layer_id + 1))
        )
        _init_normal(layer.mlp.down_proj, std=ff_out_std)

        nn.init.ones_(layer.input_layernorm.weight)
        nn.init.ones_(layer.post_attention_layernorm.weight)

    nn.init.ones_(model.model.norm.weight)

    if model.lm_head is not None:
        lm_std = 1 / math.sqrt(model.config.hidden_size)
        _init_normal(model.lm_head, std=lm_std)


def main(args):
    if not args.config:
        raise RuntimeError("No config provided")
    config = LlamaConfig.from_pretrained(args.config)
    torch.set_default_dtype(args.dtype)
    if args.device:
        torch.set_default_device(args.device)
    print("Initializing random model...")
    model = LlamaForCausalLM(config)
    print("OLMo initialization...")
    initialize_weights(model)
    print(model)
    total_params = sum(p.numel() for p in model.parameters())
    print(f"Model has {total_params} parameters")
    if not args.dry_run:
        if not args.repo:
            raise RuntimeError("No repo provided")
        model.push_to_hub(args.repo, private=args.private)
        if args.tokenizer:
            AutoTokenizer.from_pretrained(args.tokenizer).push_to_hub(
                args.repo, private=args.private
            )


args = argparse.ArgumentParser()
args.add_argument(
    "--config",
    type=str,
    help="source config repo or path to JSON config",
)
args.add_argument("--repo", type=str, help="destination repo")
args.add_argument("--private", action="store_true", help="push as a private repo")
args.add_argument("--dtype", type=int, default=torch.bfloat16, help="torch dtype")
args.add_argument("--dry-run", action="store_true", help="don't actually push")
args.add_argument("--device", type=str, help="device to init on")
args.add_argument("--tokenizer", type=str, help="tokenizer")

main(args.parse_args())
