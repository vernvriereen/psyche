import torch
import sys
import json
import numpy as np

from einops import rearrange

# Taken from consilience
class CompressDCT:
    @torch.no_grad()
    def __init__(self, topk=1, randk=0):
        self.topk = topk
        self.randk = randk

        if topk == 0 and randk == 0:
            topk = 1

    @torch.no_grad()
    def compress(self, x):
        xshape = x.shape
        if len(x.shape) > 2:  # 2D weights
            n1 = x.shape[1]
            n2 = x.shape[3]
            x = rearrange(x, "y h x w -> y x (h w)")

        totalk = x.shape[-1]
        topk = min(self.topk, totalk)
        remainingk = totalk - topk
        randk = min(self.randk, remainingk)

        all_idx = None

        if topk > 0:
            top_idx = torch.topk(
                x.abs(), k=topk, dim=-1, largest=True, sorted=False
            ).indices

        if randk > 0:
            rand_x = torch.rand_like(x)
            if topk > 0:
                rand_x.scatter_(dim=-1, index=top_idx, value=-1)
            rand_idx = torch.topk(
                rand_x, k=randk, dim=-1, largest=True, sorted=False
            ).indices

        if randk > 0 and topk > 0:
            all_idx = torch.concatenate([top_idx, rand_idx], dim=-1)
        elif topk > 0:
            all_idx = top_idx
        elif randk > 0:
            all_idx = rand_idx

        val = torch.gather(x, dim=-1, index=all_idx)

        return all_idx.to(dtype=torch.int32), val, xshape

    @torch.no_grad()
    def decompress(self, idx, val, xshape):
        x = torch.zeros(xshape, device=val.device, dtype=val.dtype)

        if len(xshape) > 2:  # 2D weights
            x = rearrange(x, "y h x w -> y x (h w)")

        # TODO: Careful, this is nondeterministic across different CUDA devices! might cause errors to accumulate between nodes!
        x.scatter_add_(dim=-1, index=idx.to(dtype=torch.int64), src=val)
        # x.scatter_reduce_(dim=-1, index=idx, src=val, reduce="mean", include_self=False)

        if len(xshape) > 2:  # 2D weights
            x = rearrange(x, "y x (h w) -> y h x w", w=xshape[-1])

        return x

    @torch.no_grad()
    def batch_decompress(self, batch, device=None, dtype=None):
        # idx, val, xshapes = zip(*batch)
        idx, val, xshapes = batch

        return self.decompress(
            torch.concatenate(idx, dim=-1).to(device=device),
            torch.concatenate(val, dim=-1).to(device=device, dtype=dtype),
            xshapes[0],
        )

def load_json_data(input_data):
    try:
        data = json.loads(input_data)
        return data
    except json.JSONDecodeError:
        print("Error: Invalid JSON format in input data.")
        sys.exit(1)
    except Exception as e:
        print(f"Error: An unexpected error occurred: {str(e)}")
        sys.exit(1)

def main():
    if len(sys.argv) < 2:
        raise ValueError(
            "Usage: cat <json_file> | python expand-distro.py <config_file_path>"
        )

    config = load_json_data(open(sys.argv[1], "r").read())
    if config["method"]["type"] != "distro":
        raise ValueError("Config method type must be distro")

    compress_topk = config["method"]["compress_topk"]
    compress_randk = config["method"]["compress_randk"]

    input_data = sys.stdin.read()
    json_data = load_json_data(input_data)

    # If json_data is an array, it must be size 1, and json_data should become the first element
    if isinstance(json_data, list):
        if len(json_data) != 1:
            raise ValueError("invalid distro data. array len must be 1.")
        json_data = json_data[0]

    if not (
        len(json_data["val"]) == len(json_data["all_idx"]) == len(json_data["shape"])
    ):
        raise ValueError("invalid distro data. lens dont match.")
    c = CompressDCT(compress_topk, compress_randk)
    
    for idx, val, shape in zip(
        json_data["all_idx"], json_data["val"], json_data["shape"]
    ):
        result = c.decompress(torch.Tensor(idx), torch.Tensor(val), shape)
        np_array = result.numpy().astype(np.float32)
        sys.stdout.buffer.write(np_array.tobytes())

if __name__ == "__main__":
    main()
